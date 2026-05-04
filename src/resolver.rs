use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::errors::CompileError;
use crate::lexer;
use crate::names::{canonical_name_for_decl, Name, NameKind};
use crate::parser;
use crate::parser::ast::{
    BinOp, CallableTarget, CatchClause, ClassMethod, Expr, ExprKind, Program, Stmt,
    StmtKind, UseKind,
};
use crate::span::Span;

#[derive(Clone, Default)]
struct ResolveState {
    constants: HashMap<String, String>,
    namespace: Option<String>,
    const_imports: HashMap<String, String>,
}

/// Resolves all include/require statements by inlining the referenced files.
/// Runs between parsing and type checking.
pub fn resolve(program: Program, base_dir: &Path) -> Result<Program, CompileError> {
    if !has_includes(&program) {
        return Ok(program);
    }

    let mut declared_once: HashSet<PathBuf> = HashSet::new();
    let mut include_chain: Vec<PathBuf> = Vec::new();
    let mut state = ResolveState::default();
    resolve_stmts(
        program,
        base_dir,
        &mut declared_once,
        &mut include_chain,
        &mut state,
    )
}

/// Fold a path expression to a compile-time string. Handles string literals,
/// concat of foldable subexpressions, and references to const/define-d string
/// constants tracked in `state`. Returns the human-readable error message when
/// the expression cannot be folded.
fn fold_include_path(expr: &Expr, state: &ResolveState) -> Result<String, String> {
    match &expr.kind {
        ExprKind::StringLiteral(s) => Ok(s.clone()),
        ExprKind::BinaryOp {
            left,
            op: BinOp::Concat,
            right,
        } => {
            let l = fold_include_path(left, state)?;
            let r = fold_include_path(right, state)?;
            Ok(l + &r)
        }
        ExprKind::ConstRef(name) => resolve_constant_ref(name, state).ok_or_else(|| {
            format!(
                "include path references unknown constant '{}'; \
                 the constant must be defined (via `const` or `define()`) \
                 before the include statement",
                name.as_str()
            )
        }),
        _ => Err(include_path_error_message(expr)),
    }
}

fn include_path_error_message(expr: &Expr) -> String {
    if let Some(detail) = runtime_dynamic_include_path_detail(expr) {
        return format!(
            "Runtime-dynamic include/require path expressions are not supported: {}. \
             Include paths must be compile-time-constant strings (string literals, \
             concatenations of foldable strings, or `const`/`define()` string constants)",
            detail
        );
    }

    format!(
        "include path must be a compile-time-constant string \
         (string literal, concatenation thereof, or a `const`/`define()`-d \
         string constant): {}",
        invalid_include_path_detail(expr)
    )
}

fn runtime_dynamic_include_path_detail(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Variable(name) => {
            Some(format!("variable `${}` is resolved at runtime", name))
        }
        ExprKind::FunctionCall { name, .. } => {
            Some(format!("function call `{}()` is resolved at runtime", name.as_str()))
        }
        ExprKind::ClosureCall { var, .. } => {
            Some(format!("closure call `${}()` is resolved at runtime", var))
        }
        ExprKind::ExprCall { .. } => {
            Some("callable expression call is resolved at runtime".to_string())
        }
        ExprKind::MethodCall { method, .. } | ExprKind::NullsafeMethodCall { method, .. } => {
            Some(format!("method call `->{}` is resolved at runtime", method))
        }
        ExprKind::StaticMethodCall { method, .. } => {
            Some(format!("static method call `::{}` is resolved at runtime", method))
        }
        ExprKind::Ternary { .. } | ExprKind::ShortTernary { .. } => {
            Some("ternary path selection is resolved at runtime".to_string())
        }
        ExprKind::PropertyAccess { property, .. } | ExprKind::NullsafePropertyAccess { property, .. } => {
            Some(format!("property access `->{}` is resolved at runtime", property))
        }
        ExprKind::StaticPropertyAccess { property, .. } => {
            Some(format!("static property access `::${}` is resolved at runtime", property))
        }
        ExprKind::ArrayAccess { .. } => {
            Some("array access is resolved at runtime".to_string())
        }
        _ => None,
    }
}

fn invalid_include_path_detail(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::BinaryOp { op, .. } if *op != BinOp::Concat => {
            "only string concatenation can be folded for include paths".to_string()
        }
        ExprKind::BinaryOp { .. } => {
            "concatenation contains a runtime-evaluated subexpression".to_string()
        }
        ExprKind::IntLiteral(_) => "integer literals are not valid include paths".to_string(),
        ExprKind::FloatLiteral(_) => "float literals are not valid include paths".to_string(),
        ExprKind::BoolLiteral(_) => "boolean literals are not valid include paths".to_string(),
        ExprKind::Null => "null is not a valid include path".to_string(),
        _ => "this expression cannot be folded to a string at compile time".to_string(),
    }
}

fn resolve_constant_ref(name: &Name, state: &ResolveState) -> Option<String> {
    constant_lookup_candidates(name, state)
        .into_iter()
        .find_map(|candidate| state.constants.get(&candidate).cloned())
}

fn constant_lookup_candidates(name: &Name, state: &ResolveState) -> Vec<String> {
    if name.is_fully_qualified() {
        return vec![name.as_canonical()];
    }

    if name.is_unqualified() {
        if let Some(alias) = name
            .last_segment()
            .and_then(|segment| state.const_imports.get(segment))
        {
            return vec![alias.clone()];
        }

        let raw = name.as_canonical();
        if let Some(namespace) = state.namespace.as_deref() {
            if !namespace.is_empty() {
                return vec![format!("{}\\{}", namespace, raw), raw];
            }
        }
        return vec![raw];
    }

    if let Some(first) = name.parts.first() {
        if let Some(alias) = state.const_imports.get(first) {
            let suffix = &name.parts[1..];
            if suffix.is_empty() {
                return vec![alias.clone()];
            }
            return vec![format!("{}\\{}", alias, suffix.join("\\"))];
        }
    }

    let raw = name.as_canonical();
    if name.kind == NameKind::Qualified {
        if let Some(namespace) = state.namespace.as_deref() {
            if !namespace.is_empty() {
                return vec![format!("{}\\{}", namespace, raw)];
            }
        }
    }
    vec![raw]
}

fn normalize_defined_constant_name(name: &str) -> String {
    name.trim_start_matches('\\').to_string()
}

fn namespace_string(name: &Option<Name>) -> String {
    name.as_ref().map(Name::as_canonical).unwrap_or_default()
}

fn register_const_imports(state: &mut ResolveState, stmt: &Stmt) {
    let StmtKind::UseDecl { imports } = &stmt.kind else {
        return;
    };
    for item in imports {
        if item.kind == UseKind::Const {
            state.const_imports.insert(
                item.alias.clone(),
                normalize_defined_constant_name(&item.name.as_canonical()),
            );
        }
    }
}

fn is_define_call_name(name: &Name) -> bool {
    matches!(name.kind, NameKind::Unqualified | NameKind::FullyQualified)
        && name.parts.len() == 1
        && name.parts[0] == "define"
}

/// Check if any statement or closure expression recursively contains an Include.
fn has_includes(stmts: &[Stmt]) -> bool {
    stmts.iter().any(stmt_has_includes)
}

fn stmt_has_includes(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Include { .. } => true,
        StmtKind::Synthetic(stmts) | StmtKind::IncludeOnceGuard { body: stmts, .. } => {
            has_includes(stmts)
        }
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::ConstDecl { value: expr, .. }
        | StmtKind::ListUnpack { value: expr, .. }
        | StmtKind::StaticVar { init: expr, .. }
        | StmtKind::Assign { value: expr, .. }
        | StmtKind::TypedAssign { value: expr, .. }
        | StmtKind::ArrayPush { value: expr, .. }
        | StmtKind::StaticPropertyAssign { value: expr, .. }
        | StmtKind::StaticPropertyArrayPush { value: expr, .. } => expr_has_includes(expr),
        StmtKind::Return(expr) => expr.as_ref().is_some_and(expr_has_includes),
        StmtKind::ArrayAssign { index, value, .. }
        | StmtKind::StaticPropertyArrayAssign { index, value, .. }
        | StmtKind::PropertyArrayAssign { index, value, .. } => {
            expr_has_includes(index) || expr_has_includes(value)
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            expr_has_includes(object) || expr_has_includes(value)
        }
        StmtKind::If { condition, then_body, elseif_clauses, else_body } => {
            expr_has_includes(condition)
                || has_includes(then_body)
                || elseif_clauses.iter().any(|(condition, body)| {
                    expr_has_includes(condition) || has_includes(body)
                })
                || else_body.as_ref().is_some_and(|body| has_includes(body))
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            expr_has_includes(condition) || has_includes(body)
        }
        StmtKind::NamespaceBlock { body, .. } => has_includes(body),
        StmtKind::FunctionDecl { params, body, .. } => {
            params.iter().any(|(_, _, default, _)| {
                default.as_ref().is_some_and(expr_has_includes)
            }) || has_includes(body)
        }
        StmtKind::Try { try_body, catches, finally_body } => {
            has_includes(try_body)
                || catches.iter().any(|catch_clause| has_includes(&catch_clause.body))
                || finally_body.as_ref().is_some_and(|body| has_includes(body))
        }
        StmtKind::ClassDecl { properties, methods, .. }
        | StmtKind::TraitDecl { properties, methods, .. } => {
            properties
                .iter()
                .any(|property| property.default.as_ref().is_some_and(expr_has_includes))
                || methods_have_includes(methods)
        }
        StmtKind::InterfaceDecl { methods, .. } => methods_have_includes(methods),
        StmtKind::Switch { subject, cases, default } => {
            expr_has_includes(subject)
                || cases.iter().any(|(values, body)| {
                    values.iter().any(expr_has_includes) || has_includes(body)
                })
                || default.as_ref().is_some_and(|body| has_includes(body))
        }
        StmtKind::Foreach { array, body, .. } => expr_has_includes(array) || has_includes(body),
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_ref().is_some_and(|stmt| stmt_has_includes(stmt))
                || condition.as_ref().is_some_and(expr_has_includes)
                || update.as_ref().is_some_and(|stmt| stmt_has_includes(stmt))
                || has_includes(body)
        }
        StmtKind::EnumDecl { cases, .. } => cases
            .iter()
            .any(|case| case.value.as_ref().is_some_and(expr_has_includes)),
        StmtKind::IncludeOnceMark { .. }
        | StmtKind::IfDef { .. }
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::Global { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => false,
    }
}

fn methods_have_includes(methods: &[ClassMethod]) -> bool {
    methods.iter().any(|method| {
        method.params.iter().any(|(_, _, default, _)| {
            default.as_ref().is_some_and(expr_has_includes)
        }) || has_includes(&method.body)
    })
}

fn expr_has_includes(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::BinaryOp { left, right, .. } => {
            expr_has_includes(left) || expr_has_includes(right)
        }
        ExprKind::InstanceOf { value, .. }
        | ExprKind::Negate(value)
        | ExprKind::Not(value)
        | ExprKind::BitNot(value)
        | ExprKind::Throw(value)
        | ExprKind::ErrorSuppress(value)
        | ExprKind::Print(value)
        | ExprKind::Spread(value)
        | ExprKind::PtrCast { expr: value, .. }
        | ExprKind::BufferNew { len: value, .. } => expr_has_includes(value),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ArrayAccess { array: value, index: default }
        | ExprKind::ShortTernary { value, default } => {
            expr_has_includes(value) || expr_has_includes(default)
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            expr_has_includes(target)
                || expr_has_includes(value)
                || result_target.as_ref().is_some_and(|expr| expr_has_includes(expr))
                || has_includes(prelude)
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. } => args.iter().any(expr_has_includes),
        ExprKind::ArrayLiteral(items) => items.iter().any(expr_has_includes),
        ExprKind::ArrayLiteralAssoc(entries) => entries
            .iter()
            .any(|(key, value)| expr_has_includes(key) || expr_has_includes(value)),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_has_includes(subject)
                || arms.iter().any(|(patterns, value)| {
                    patterns.iter().any(expr_has_includes) || expr_has_includes(value)
                })
                || default.as_ref().is_some_and(|expr| expr_has_includes(expr))
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_has_includes(condition)
                || expr_has_includes(then_expr)
                || expr_has_includes(else_expr)
        }
        ExprKind::Cast { expr, .. } => expr_has_includes(expr),
        ExprKind::Closure { params, body, .. } => {
            params.iter().any(|(_, _, default, _)| {
                default.as_ref().is_some_and(expr_has_includes)
            }) || has_includes(body)
        }
        ExprKind::NamedArg { value, .. } => expr_has_includes(value),
        ExprKind::ExprCall { callee, args } => {
            expr_has_includes(callee) || args.iter().any(expr_has_includes)
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => expr_has_includes(object),
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_has_includes(object) || args.iter().any(expr_has_includes)
        }
        ExprKind::FirstClassCallable(CallableTarget::Method { object, .. }) => {
            expr_has_includes(object)
        }
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::Variable(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::ConstRef(_)
        | ExprKind::EnumCase { .. }
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::FirstClassCallable(_)
        | ExprKind::This
        | ExprKind::ClassConstant { .. }
        | ExprKind::MagicConstant(_) => false,
    }
}

fn resolve_stmt_exprs(
    stmt: Stmt,
    base_dir: &Path,
    declared_once: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
) -> Result<Stmt, CompileError> {
    let span = stmt.span;
    let kind = match stmt.kind {
        StmtKind::Synthetic(stmts) => StmtKind::Synthetic(resolve_isolated(
            stmts,
            base_dir,
            declared_once,
            include_chain,
            state,
        )?),
        StmtKind::IncludeOnceMark { label } => StmtKind::IncludeOnceMark { label },
        StmtKind::IncludeOnceGuard { label, body } => StmtKind::IncludeOnceGuard {
            label,
            body: resolve_isolated(body, base_dir, declared_once, include_chain, state)?,
        },
        StmtKind::Echo(expr) => StmtKind::Echo(resolve_expr(
            expr,
            base_dir,
            declared_once,
            include_chain,
            state,
        )?),
        StmtKind::Throw(expr) => StmtKind::Throw(resolve_expr(
            expr,
            base_dir,
            declared_once,
            include_chain,
            state,
        )?),
        StmtKind::ExprStmt(expr) => StmtKind::ExprStmt(resolve_expr(
            expr,
            base_dir,
            declared_once,
            include_chain,
            state,
        )?),
        StmtKind::Return(expr) => StmtKind::Return(
            expr.map(|expr| resolve_expr(expr, base_dir, declared_once, include_chain, state))
                .transpose()?,
        ),
        StmtKind::Assign { name, value } => StmtKind::Assign {
            name,
            value: resolve_expr(value, base_dir, declared_once, include_chain, state)?,
        },
        StmtKind::TypedAssign {
            type_expr,
            name,
            value,
        } => StmtKind::TypedAssign {
            type_expr,
            name,
            value: resolve_expr(value, base_dir, declared_once, include_chain, state)?,
        },
        StmtKind::ConstDecl { name, value } => StmtKind::ConstDecl {
            name,
            value: resolve_expr(value, base_dir, declared_once, include_chain, state)?,
        },
        StmtKind::ListUnpack { vars, value } => StmtKind::ListUnpack {
            vars,
            value: resolve_expr(value, base_dir, declared_once, include_chain, state)?,
        },
        StmtKind::StaticVar { name, init } => StmtKind::StaticVar {
            name,
            init: resolve_expr(init, base_dir, declared_once, include_chain, state)?,
        },
        StmtKind::ArrayAssign {
            array,
            index,
            value,
        } => StmtKind::ArrayAssign {
            array,
            index: resolve_expr(index, base_dir, declared_once, include_chain, state)?,
            value: resolve_expr(value, base_dir, declared_once, include_chain, state)?,
        },
        StmtKind::ArrayPush { array, value } => StmtKind::ArrayPush {
            array,
            value: resolve_expr(value, base_dir, declared_once, include_chain, state)?,
        },
        StmtKind::PropertyAssign {
            object,
            property,
            value,
        } => StmtKind::PropertyAssign {
            object: Box::new(resolve_expr(
                *object,
                base_dir,
                declared_once,
                include_chain,
                state,
            )?),
            property,
            value: resolve_expr(value, base_dir, declared_once, include_chain, state)?,
        },
        StmtKind::PropertyArrayPush {
            object,
            property,
            value,
        } => StmtKind::PropertyArrayPush {
            object: Box::new(resolve_expr(
                *object,
                base_dir,
                declared_once,
                include_chain,
                state,
            )?),
            property,
            value: resolve_expr(value, base_dir, declared_once, include_chain, state)?,
        },
        StmtKind::PropertyArrayAssign {
            object,
            property,
            index,
            value,
        } => StmtKind::PropertyArrayAssign {
            object: Box::new(resolve_expr(
                *object,
                base_dir,
                declared_once,
                include_chain,
                state,
            )?),
            property,
            index: resolve_expr(index, base_dir, declared_once, include_chain, state)?,
            value: resolve_expr(value, base_dir, declared_once, include_chain, state)?,
        },
        StmtKind::StaticPropertyAssign {
            receiver,
            property,
            value,
        } => StmtKind::StaticPropertyAssign {
            receiver,
            property,
            value: resolve_expr(value, base_dir, declared_once, include_chain, state)?,
        },
        StmtKind::StaticPropertyArrayPush {
            receiver,
            property,
            value,
        } => StmtKind::StaticPropertyArrayPush {
            receiver,
            property,
            value: resolve_expr(value, base_dir, declared_once, include_chain, state)?,
        },
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            property,
            index,
            value,
        } => StmtKind::StaticPropertyArrayAssign {
            receiver,
            property,
            index: resolve_expr(index, base_dir, declared_once, include_chain, state)?,
            value: resolve_expr(value, base_dir, declared_once, include_chain, state)?,
        },
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => StmtKind::If {
            condition: resolve_expr(condition, base_dir, declared_once, include_chain, state)?,
            then_body,
            elseif_clauses: elseif_clauses
                .into_iter()
                .map(|(condition, body)| {
                    Ok((
                        resolve_expr(condition, base_dir, declared_once, include_chain, state)?,
                        body,
                    ))
                })
                .collect::<Result<Vec<_>, CompileError>>()?,
            else_body,
        },
        StmtKind::While { condition, body } => StmtKind::While {
            condition: resolve_expr(condition, base_dir, declared_once, include_chain, state)?,
            body,
        },
        StmtKind::DoWhile { body, condition } => StmtKind::DoWhile {
            body,
            condition: resolve_expr(condition, base_dir, declared_once, include_chain, state)?,
        },
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => StmtKind::For {
            init: init
                .map(|stmt| {
                    resolve_stmt_exprs(*stmt, base_dir, declared_once, include_chain, state)
                        .map(Box::new)
                })
                .transpose()?,
            condition: condition
                .map(|expr| resolve_expr(expr, base_dir, declared_once, include_chain, state))
                .transpose()?,
            update: update
                .map(|stmt| {
                    resolve_stmt_exprs(*stmt, base_dir, declared_once, include_chain, state)
                        .map(Box::new)
                })
                .transpose()?,
            body,
        },
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            body,
        } => StmtKind::Foreach {
            array: resolve_expr(array, base_dir, declared_once, include_chain, state)?,
            key_var,
            value_var,
            body,
        },
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => StmtKind::Switch {
            subject: resolve_expr(subject, base_dir, declared_once, include_chain, state)?,
            cases: cases
                .into_iter()
                .map(|(values, body)| {
                    Ok((
                        values
                            .into_iter()
                            .map(|value| {
                                resolve_expr(
                                    value,
                                    base_dir,
                                    declared_once,
                                    include_chain,
                                    state,
                                )
                            })
                            .collect::<Result<Vec<_>, CompileError>>()?,
                        body,
                    ))
                })
                .collect::<Result<Vec<_>, CompileError>>()?,
            default,
        },
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => StmtKind::Try {
            try_body,
            catches,
            finally_body,
        },
        StmtKind::FunctionDecl {
            name,
            params,
            variadic,
            return_type,
            body,
        } => StmtKind::FunctionDecl {
            name,
            params: resolve_params(params, base_dir, declared_once, include_chain, state)?,
            variadic,
            return_type,
            body,
        },
        StmtKind::ClassDecl {
            name,
            extends,
            implements,
            is_abstract,
            is_final,
            is_readonly_class,
            trait_uses,
            properties,
            methods,
        } => StmtKind::ClassDecl {
            name,
            extends,
            implements,
            is_abstract,
            is_final,
            is_readonly_class,
            trait_uses,
            properties: resolve_properties(
                properties,
                base_dir,
                declared_once,
                include_chain,
                state,
            )?,
            methods: resolve_method_exprs(methods, base_dir, declared_once, include_chain, state)?,
        },
        StmtKind::InterfaceDecl {
            name,
            extends,
            methods,
        } => StmtKind::InterfaceDecl {
            name,
            extends,
            methods: resolve_method_exprs(methods, base_dir, declared_once, include_chain, state)?,
        },
        StmtKind::TraitDecl {
            name,
            trait_uses,
            properties,
            methods,
        } => StmtKind::TraitDecl {
            name,
            trait_uses,
            properties: resolve_properties(
                properties,
                base_dir,
                declared_once,
                include_chain,
                state,
            )?,
            methods: resolve_method_exprs(methods, base_dir, declared_once, include_chain, state)?,
        },
        StmtKind::EnumDecl {
            name,
            backing_type,
            cases,
        } => StmtKind::EnumDecl {
            name,
            backing_type,
            cases: cases
                .into_iter()
                .map(|mut case| {
                    case.value = case
                        .value
                        .map(|expr| {
                            resolve_expr(expr, base_dir, declared_once, include_chain, state)
                        })
                        .transpose()?;
                    Ok(case)
                })
                .collect::<Result<Vec<_>, CompileError>>()?,
        },
        StmtKind::NamespaceBlock { name, body } => StmtKind::NamespaceBlock { name, body },
        StmtKind::Include {
            path,
            once,
            required,
        } => StmtKind::Include {
            path,
            once,
            required,
        },
        other @ (StmtKind::IfDef { .. }
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::Global { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. }) => other,
    };
    Ok(Stmt::new(kind, span))
}

fn resolve_expr(
    expr: Expr,
    base_dir: &Path,
    declared_once: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
) -> Result<Expr, CompileError> {
    let span = expr.span;
    let kind = match expr.kind {
        ExprKind::BinaryOp { left, op, right } => ExprKind::BinaryOp {
            left: Box::new(resolve_expr(*left, base_dir, declared_once, include_chain, state)?),
            op,
            right: Box::new(resolve_expr(*right, base_dir, declared_once, include_chain, state)?),
        },
        ExprKind::InstanceOf { value, target } => ExprKind::InstanceOf {
            value: Box::new(resolve_expr(*value, base_dir, declared_once, include_chain, state)?),
            target,
        },
        ExprKind::Negate(inner) => ExprKind::Negate(Box::new(resolve_expr(
            *inner,
            base_dir,
            declared_once,
            include_chain,
            state,
        )?)),
        ExprKind::Not(inner) => ExprKind::Not(Box::new(resolve_expr(
            *inner,
            base_dir,
            declared_once,
            include_chain,
            state,
        )?)),
        ExprKind::BitNot(inner) => ExprKind::BitNot(Box::new(resolve_expr(
            *inner,
            base_dir,
            declared_once,
            include_chain,
            state,
        )?)),
        ExprKind::Throw(inner) => ExprKind::Throw(Box::new(resolve_expr(
            *inner,
            base_dir,
            declared_once,
            include_chain,
            state,
        )?)),
        ExprKind::ErrorSuppress(inner) => ExprKind::ErrorSuppress(Box::new(resolve_expr(
            *inner,
            base_dir,
            declared_once,
            include_chain,
            state,
        )?)),
        ExprKind::Print(inner) => ExprKind::Print(Box::new(resolve_expr(
            *inner,
            base_dir,
            declared_once,
            include_chain,
            state,
        )?)),
        ExprKind::NullCoalesce { value, default } => ExprKind::NullCoalesce {
            value: Box::new(resolve_expr(*value, base_dir, declared_once, include_chain, state)?),
            default: Box::new(resolve_expr(*default, base_dir, declared_once, include_chain, state)?),
        },
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            conditional_value_temp,
        } => ExprKind::Assignment {
            target: Box::new(resolve_expr(*target, base_dir, declared_once, include_chain, state)?),
            value: Box::new(resolve_expr(*value, base_dir, declared_once, include_chain, state)?),
            result_target: result_target
                .map(|target| resolve_expr(*target, base_dir, declared_once, include_chain, state))
                .transpose()?
                .map(Box::new),
            prelude: resolve_isolated(prelude, base_dir, declared_once, include_chain, state)?,
            conditional_value_temp,
        },
        ExprKind::FunctionCall { name, args } => ExprKind::FunctionCall {
            name,
            args: resolve_exprs(args, base_dir, declared_once, include_chain, state)?,
        },
        ExprKind::ArrayLiteral(items) => ExprKind::ArrayLiteral(resolve_exprs(
            items,
            base_dir,
            declared_once,
            include_chain,
            state,
        )?),
        ExprKind::ArrayLiteralAssoc(entries) => ExprKind::ArrayLiteralAssoc(
            entries
                .into_iter()
                .map(|(key, value)| {
                    Ok((
                        resolve_expr(key, base_dir, declared_once, include_chain, state)?,
                        resolve_expr(value, base_dir, declared_once, include_chain, state)?,
                    ))
                })
                .collect::<Result<Vec<_>, CompileError>>()?,
        ),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => ExprKind::Match {
            subject: Box::new(resolve_expr(
                *subject,
                base_dir,
                declared_once,
                include_chain,
                state,
            )?),
            arms: arms
                .into_iter()
                .map(|(patterns, value)| {
                    Ok((
                        resolve_exprs(patterns, base_dir, declared_once, include_chain, state)?,
                        resolve_expr(value, base_dir, declared_once, include_chain, state)?,
                    ))
                })
                .collect::<Result<Vec<_>, CompileError>>()?,
            default: default
                .map(|expr| resolve_expr(*expr, base_dir, declared_once, include_chain, state))
                .transpose()?
                .map(Box::new),
        },
        ExprKind::ArrayAccess { array, index } => ExprKind::ArrayAccess {
            array: Box::new(resolve_expr(*array, base_dir, declared_once, include_chain, state)?),
            index: Box::new(resolve_expr(*index, base_dir, declared_once, include_chain, state)?),
        },
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => ExprKind::Ternary {
            condition: Box::new(resolve_expr(
                *condition,
                base_dir,
                declared_once,
                include_chain,
                state,
            )?),
            then_expr: Box::new(resolve_expr(
                *then_expr,
                base_dir,
                declared_once,
                include_chain,
                state,
            )?),
            else_expr: Box::new(resolve_expr(
                *else_expr,
                base_dir,
                declared_once,
                include_chain,
                state,
            )?),
        },
        ExprKind::ShortTernary { value, default } => ExprKind::ShortTernary {
            value: Box::new(resolve_expr(*value, base_dir, declared_once, include_chain, state)?),
            default: Box::new(resolve_expr(*default, base_dir, declared_once, include_chain, state)?),
        },
        ExprKind::Cast { target, expr } => ExprKind::Cast {
            target,
            expr: Box::new(resolve_expr(*expr, base_dir, declared_once, include_chain, state)?),
        },
        ExprKind::Closure {
            params,
            variadic,
            return_type,
            body,
            is_arrow,
            is_static,
            captures,
        } => ExprKind::Closure {
            params: resolve_params(params, base_dir, declared_once, include_chain, state)?,
            variadic,
            return_type,
            body: resolve_isolated(body, base_dir, declared_once, include_chain, state)?,
            is_arrow,
            is_static,
            captures,
        },
        ExprKind::NamedArg { name, value } => ExprKind::NamedArg {
            name,
            value: Box::new(resolve_expr(*value, base_dir, declared_once, include_chain, state)?),
        },
        ExprKind::Spread(inner) => ExprKind::Spread(Box::new(resolve_expr(
            *inner,
            base_dir,
            declared_once,
            include_chain,
            state,
        )?)),
        ExprKind::ClosureCall { var, args } => ExprKind::ClosureCall {
            var,
            args: resolve_exprs(args, base_dir, declared_once, include_chain, state)?,
        },
        ExprKind::ExprCall { callee, args } => ExprKind::ExprCall {
            callee: Box::new(resolve_expr(
                *callee,
                base_dir,
                declared_once,
                include_chain,
                state,
            )?),
            args: resolve_exprs(args, base_dir, declared_once, include_chain, state)?,
        },
        ExprKind::NewObject { class_name, args } => ExprKind::NewObject {
            class_name,
            args: resolve_exprs(args, base_dir, declared_once, include_chain, state)?,
        },
        ExprKind::PropertyAccess { object, property } => ExprKind::PropertyAccess {
            object: Box::new(resolve_expr(
                *object,
                base_dir,
                declared_once,
                include_chain,
                state,
            )?),
            property,
        },
        ExprKind::NullsafePropertyAccess { object, property } => {
            ExprKind::NullsafePropertyAccess {
                object: Box::new(resolve_expr(
                    *object,
                    base_dir,
                    declared_once,
                    include_chain,
                    state,
                )?),
                property,
            }
        }
        ExprKind::MethodCall { object, method, args } => ExprKind::MethodCall {
            object: Box::new(resolve_expr(
                *object,
                base_dir,
                declared_once,
                include_chain,
                state,
            )?),
            method,
            args: resolve_exprs(args, base_dir, declared_once, include_chain, state)?,
        },
        ExprKind::NullsafeMethodCall {
            object,
            method,
            args,
        } => ExprKind::NullsafeMethodCall {
            object: Box::new(resolve_expr(
                *object,
                base_dir,
                declared_once,
                include_chain,
                state,
            )?),
            method,
            args: resolve_exprs(args, base_dir, declared_once, include_chain, state)?,
        },
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => ExprKind::StaticMethodCall {
            receiver,
            method,
            args: resolve_exprs(args, base_dir, declared_once, include_chain, state)?,
        },
        ExprKind::FirstClassCallable(target) => {
            ExprKind::FirstClassCallable(resolve_callable_target(
                target,
                base_dir,
                declared_once,
                include_chain,
                state,
            )?)
        }
        ExprKind::PtrCast { target_type, expr } => ExprKind::PtrCast {
            target_type,
            expr: Box::new(resolve_expr(*expr, base_dir, declared_once, include_chain, state)?),
        },
        ExprKind::BufferNew { element_type, len } => ExprKind::BufferNew {
            element_type,
            len: Box::new(resolve_expr(*len, base_dir, declared_once, include_chain, state)?),
        },
        ExprKind::NewScopedObject { receiver, args } => ExprKind::NewScopedObject {
            receiver,
            args: resolve_exprs(args, base_dir, declared_once, include_chain, state)?,
        },
        other => other,
    };
    Ok(Expr::new(kind, span))
}

fn resolve_exprs(
    exprs: Vec<Expr>,
    base_dir: &Path,
    declared_once: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
) -> Result<Vec<Expr>, CompileError> {
    exprs
        .into_iter()
        .map(|expr| resolve_expr(expr, base_dir, declared_once, include_chain, state))
        .collect()
}

fn resolve_params(
    params: Vec<(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)>,
    base_dir: &Path,
    declared_once: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
) -> Result<Vec<(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)>, CompileError> {
    params
        .into_iter()
        .map(|(name, type_expr, default, is_ref)| {
            Ok((
                name,
                type_expr,
                default
                    .map(|expr| resolve_expr(expr, base_dir, declared_once, include_chain, state))
                    .transpose()?,
                is_ref,
            ))
        })
        .collect()
}

fn resolve_properties(
    properties: Vec<crate::parser::ast::ClassProperty>,
    base_dir: &Path,
    declared_once: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
) -> Result<Vec<crate::parser::ast::ClassProperty>, CompileError> {
    properties
        .into_iter()
        .map(|mut property| {
            property.default = property
                .default
                .map(|expr| resolve_expr(expr, base_dir, declared_once, include_chain, state))
                .transpose()?;
            Ok(property)
        })
        .collect()
}

fn resolve_method_exprs(
    methods: Vec<ClassMethod>,
    base_dir: &Path,
    declared_once: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
) -> Result<Vec<ClassMethod>, CompileError> {
    methods
        .into_iter()
        .map(|mut method| {
            method.params = resolve_params(method.params, base_dir, declared_once, include_chain, state)?;
            Ok(method)
        })
        .collect()
}

fn resolve_callable_target(
    target: CallableTarget,
    base_dir: &Path,
    declared_once: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
) -> Result<CallableTarget, CompileError> {
    Ok(match target {
        CallableTarget::Function(name) => CallableTarget::Function(name),
        CallableTarget::StaticMethod { receiver, method } => {
            CallableTarget::StaticMethod { receiver, method }
        }
        CallableTarget::Method { object, method } => CallableTarget::Method {
            object: Box::new(resolve_expr(
                *object,
                base_dir,
                declared_once,
                include_chain,
                state,
            )?),
            method,
        },
    })
}

fn resolve_stmts(
    stmts: Vec<Stmt>,
    base_dir: &Path,
    declared_once: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &mut ResolveState,
) -> Result<Vec<Stmt>, CompileError> {
    let mut result = Vec::new();

    for stmt in stmts {
        let stmt = resolve_stmt_exprs(stmt, base_dir, declared_once, include_chain, state)?;
        match &stmt.kind {
            StmtKind::Include { path, once, required } => {
                let path_str = fold_include_path(path, state)
                    .map_err(|msg| CompileError::new(stmt.span, &msg))?;
                let resolved = resolve_path(&path_str, base_dir);
                let canonical = resolved.canonicalize().unwrap_or_else(|_| resolved.clone());

                if !resolved.exists() {
                    if *required {
                        return Err(CompileError::new(
                            stmt.span,
                            &format!("Required file not found: '{}'", path_str),
                        ));
                    }
                    continue;
                }

                if include_chain.contains(&canonical) {
                    if *once {
                        continue;
                    }
                    return Err(CompileError::new(
                        stmt.span,
                        &format!("Circular include detected: '{}'", path_str),
                    ));
                }

                let included_stmts = parse_file(&resolved, stmt.span)?;
                let included_stmts =
                    crate::magic_constants::substitute_file_and_scope_constants(included_stmts, &resolved);

                let included_dir = resolved.parent().unwrap_or(base_dir);
                include_chain.push(canonical.clone());

                let saved_namespace = state.namespace.clone();
                let saved_imports = state.const_imports.clone();
                state.namespace = None;
                state.const_imports = HashMap::new();
                let resolved_stmts =
                    resolve_stmts(included_stmts, included_dir, declared_once, include_chain, state)?;
                state.namespace = saved_namespace;
                state.const_imports = saved_imports;

                include_chain.pop();

                let include_label = include_once_label(&canonical);
                if *once {
                    // Declarations stay hoisted for the existing AOT symbol model; executable
                    // include body statements are guarded so runtime order matches PHP.
                    let (decls, executable) = split_include_once_declarations(resolved_stmts);
                    if declared_once.insert(canonical) && !decls.is_empty() {
                        result.push(Stmt::new(
                            StmtKind::NamespaceBlock {
                                name: None,
                                body: decls,
                            },
                            stmt.span,
                        ));
                    }
                    result.push(Stmt::new(
                        StmtKind::IncludeOnceGuard {
                            label: include_label,
                            body: vec![Stmt::new(
                                StmtKind::NamespaceBlock {
                                    name: None,
                                    body: executable,
                                },
                                stmt.span,
                            )],
                        },
                        stmt.span,
                    ));
                } else {
                    // Regular includes emit declarations too. Track that so a later
                    // include_once/require_once of the same file does not hoist duplicate
                    // declarations, while the runtime mark below still happens only when
                    // execution reaches this include.
                    declared_once.insert(canonical);
                    result.push(Stmt::new(
                        StmtKind::IncludeOnceMark {
                            label: include_label,
                        },
                        stmt.span,
                    ));
                    result.push(Stmt::new(
                        StmtKind::NamespaceBlock {
                            name: None,
                            body: resolved_stmts,
                        },
                        stmt.span,
                    ));
                }
            }
            StmtKind::ConstDecl { name, value } => {
                if let Ok(s) = fold_include_path(value, state) {
                    let key = canonical_name_for_decl(state.namespace.as_deref(), name);
                    state.constants.insert(key, s);
                }
                result.push(stmt);
            }
            StmtKind::ExprStmt(expr) => {
                if let ExprKind::FunctionCall { name, args } = &expr.kind {
                    if is_define_call_name(name) && args.len() == 2 {
                        if let ExprKind::StringLiteral(const_name) = &args[0].kind {
                            if let Ok(value) = fold_include_path(&args[1], state) {
                                state
                                    .constants
                                    .insert(normalize_defined_constant_name(const_name), value);
                            }
                        }
                    }
                }
                result.push(stmt);
            }
            StmtKind::NamespaceDecl { name } => {
                state.namespace = Some(namespace_string(name));
                state.const_imports = HashMap::new();
                result.push(stmt);
            }
            StmtKind::NamespaceBlock { name, body } => {
                let saved_namespace = state.namespace.clone();
                let saved_imports = state.const_imports.clone();
                state.namespace = Some(namespace_string(name));
                state.const_imports = HashMap::new();
                let body_resolved =
                    resolve_stmts(body.clone(), base_dir, declared_once, include_chain, state)?;
                state.namespace = saved_namespace;
                state.const_imports = saved_imports;
                result.push(Stmt::new(
                    StmtKind::NamespaceBlock {
                        name: name.clone(),
                        body: body_resolved,
                    },
                    stmt.span,
                ));
            }
            StmtKind::UseDecl { .. } => {
                register_const_imports(state, &stmt);
                result.push(stmt);
            }
            StmtKind::If { condition, then_body, elseif_clauses, else_body } => {
                let then_body = resolve_isolated(then_body.clone(), base_dir, declared_once, include_chain, state)?;
                let elseif_clauses = elseif_clauses
                    .iter()
                    .map(|(cond, body)| {
                        Ok((
                            cond.clone(),
                            resolve_isolated(body.clone(), base_dir, declared_once, include_chain, state)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, CompileError>>()?;
                let else_body = else_body
                    .as_ref()
                    .map(|body| resolve_isolated(body.clone(), base_dir, declared_once, include_chain, state))
                    .transpose()?;
                result.push(Stmt::new(
                    StmtKind::If {
                        condition: condition.clone(),
                        then_body,
                        elseif_clauses,
                        else_body,
                    },
                    stmt.span,
                ));
            }
            StmtKind::While { condition, body } => {
                let body = resolve_isolated(body.clone(), base_dir, declared_once, include_chain, state)?;
                result.push(Stmt::new(
                    StmtKind::While {
                        condition: condition.clone(),
                        body,
                    },
                    stmt.span,
                ));
            }
            StmtKind::DoWhile { body, condition } => {
                let body = resolve_isolated(body.clone(), base_dir, declared_once, include_chain, state)?;
                result.push(Stmt::new(
                    StmtKind::DoWhile {
                        body,
                        condition: condition.clone(),
                    },
                    stmt.span,
                ));
            }
            StmtKind::For { init, condition, update, body } => {
                let body = resolve_isolated(body.clone(), base_dir, declared_once, include_chain, state)?;
                result.push(Stmt::new(
                    StmtKind::For {
                        init: init.clone(),
                        condition: condition.clone(),
                        update: update.clone(),
                        body,
                    },
                    stmt.span,
                ));
            }
            StmtKind::Foreach { array, key_var, value_var, body } => {
                let body = resolve_isolated(body.clone(), base_dir, declared_once, include_chain, state)?;
                result.push(Stmt::new(
                    StmtKind::Foreach {
                        array: array.clone(),
                        key_var: key_var.clone(),
                        value_var: value_var.clone(),
                        body,
                    },
                    stmt.span,
                ));
            }
            StmtKind::Switch { subject, cases, default } => {
                let cases = cases
                    .iter()
                    .map(|(values, body)| {
                        Ok((
                            values.clone(),
                            resolve_isolated(body.clone(), base_dir, declared_once, include_chain, state)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, CompileError>>()?;
                let default = default
                    .as_ref()
                    .map(|body| resolve_isolated(body.clone(), base_dir, declared_once, include_chain, state))
                    .transpose()?;
                result.push(Stmt::new(
                    StmtKind::Switch {
                        subject: subject.clone(),
                        cases,
                        default,
                    },
                    stmt.span,
                ));
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                let try_body =
                    resolve_isolated(try_body.clone(), base_dir, declared_once, include_chain, state)?;
                let catches = catches
                    .iter()
                    .map(|catch_clause| {
                        Ok(CatchClause {
                            exception_types: catch_clause.exception_types.clone(),
                            variable: catch_clause.variable.clone(),
                            body: resolve_isolated(
                                catch_clause.body.clone(),
                                base_dir,
                                declared_once,
                                include_chain,
                                state,
                            )?,
                        })
                    })
                    .collect::<Result<Vec<_>, CompileError>>()?;
                let finally_body = finally_body
                    .as_ref()
                    .map(|body| resolve_isolated(body.clone(), base_dir, declared_once, include_chain, state))
                    .transpose()?;
                result.push(Stmt::new(
                    StmtKind::Try {
                        try_body,
                        catches,
                        finally_body,
                    },
                    stmt.span,
                ));
            }
            StmtKind::FunctionDecl { name, params, variadic, return_type, body } => {
                let body = resolve_isolated(body.clone(), base_dir, declared_once, include_chain, state)?;
                result.push(Stmt::new(
                    StmtKind::FunctionDecl {
                        name: name.clone(),
                        params: params.clone(),
                        variadic: variadic.clone(),
                        return_type: return_type.clone(),
                        body,
                    },
                    stmt.span,
                ));
            }
            StmtKind::ClassDecl {
                name,
                extends,
                implements,
                is_abstract,
                is_final,
                is_readonly_class,
                trait_uses,
                properties,
                methods,
            } => {
                let methods = resolve_methods(methods, base_dir, declared_once, include_chain, state)?;
                result.push(Stmt::new(
                    StmtKind::ClassDecl {
                        name: name.clone(),
                        extends: extends.clone(),
                        implements: implements.clone(),
                        is_abstract: *is_abstract,
                        is_final: *is_final,
                        is_readonly_class: *is_readonly_class,
                        trait_uses: trait_uses.clone(),
                        properties: properties.clone(),
                        methods,
                    },
                    stmt.span,
                ));
            }
            StmtKind::InterfaceDecl { name, extends, methods } => {
                let methods = resolve_methods(methods, base_dir, declared_once, include_chain, state)?;
                result.push(Stmt::new(
                    StmtKind::InterfaceDecl {
                        name: name.clone(),
                        extends: extends.clone(),
                        methods,
                    },
                    stmt.span,
                ));
            }
            StmtKind::TraitDecl {
                name,
                trait_uses,
                properties,
                methods,
            } => {
                let methods = resolve_methods(methods, base_dir, declared_once, include_chain, state)?;
                result.push(Stmt::new(
                    StmtKind::TraitDecl {
                        name: name.clone(),
                        trait_uses: trait_uses.clone(),
                        properties: properties.clone(),
                        methods,
                    },
                    stmt.span,
                ));
            }
            _ => {
                result.push(stmt);
            }
        }
    }

    Ok(result)
}

fn resolve_isolated(
    stmts: Vec<Stmt>,
    base_dir: &Path,
    declared_once: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
) -> Result<Vec<Stmt>, CompileError> {
    let mut local = state.clone();
    resolve_stmts(stmts, base_dir, declared_once, include_chain, &mut local)
}

fn resolve_methods(
    methods: &[ClassMethod],
    base_dir: &Path,
    declared_once: &mut HashSet<PathBuf>,
    include_chain: &mut Vec<PathBuf>,
    state: &ResolveState,
) -> Result<Vec<ClassMethod>, CompileError> {
    methods
        .iter()
        .map(|method| {
            let body =
                resolve_isolated(method.body.clone(), base_dir, declared_once, include_chain, state)?;
            Ok(ClassMethod {
                body,
                ..method.clone()
            })
        })
        .collect()
}

fn include_once_label(path: &Path) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in path.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("_include_once_{hash:016x}")
}

fn split_include_once_declarations(stmts: Vec<Stmt>) -> (Vec<Stmt>, Vec<Stmt>) {
    let mut declarations = Vec::new();
    let mut executable = Vec::new();

    for stmt in stmts {
        match &stmt.kind {
            StmtKind::NamespaceDecl { .. } | StmtKind::UseDecl { .. } => {
                declarations.push(stmt.clone());
                executable.push(stmt);
            }
            StmtKind::NamespaceBlock { name, body } => {
                let (body_decls, body_exec) = split_include_once_declarations(body.clone());
                if !body_decls.is_empty() {
                    declarations.push(Stmt::new(
                        StmtKind::NamespaceBlock {
                            name: name.clone(),
                            body: body_decls,
                        },
                        stmt.span,
                    ));
                }
                if !body_exec.is_empty() {
                    executable.push(Stmt::new(
                        StmtKind::NamespaceBlock {
                            name: name.clone(),
                            body: body_exec,
                        },
                        stmt.span,
                    ));
                }
            }
            StmtKind::Synthetic(stmts) => {
                let (body_decls, body_exec) = split_include_once_declarations(stmts.clone());
                if !body_decls.is_empty() {
                    declarations.push(Stmt::new(StmtKind::Synthetic(body_decls), stmt.span));
                }
                if !body_exec.is_empty() {
                    executable.push(Stmt::new(StmtKind::Synthetic(body_exec), stmt.span));
                }
            }
            StmtKind::FunctionDecl { .. }
            | StmtKind::ClassDecl { .. }
            | StmtKind::EnumDecl { .. }
            | StmtKind::InterfaceDecl { .. }
            | StmtKind::TraitDecl { .. }
            | StmtKind::PackedClassDecl { .. }
            | StmtKind::ExternFunctionDecl { .. }
            | StmtKind::ExternClassDecl { .. }
            | StmtKind::ExternGlobalDecl { .. }
            | StmtKind::ConstDecl { .. } => declarations.push(stmt),
            _ => executable.push(stmt),
        }
    }

    (declarations, executable)
}

fn resolve_path(path: &str, base_dir: &Path) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base_dir.join(p)
    }
}

fn parse_file(path: &Path, include_span: Span) -> Result<Vec<Stmt>, CompileError> {
    let source = std::fs::read_to_string(path).map_err(|e| {
        CompileError::new(
            include_span,
            &format!("Cannot read '{}': {}", path.display(), e),
        )
    })?;

    let file = path.display().to_string();

    let tokens = lexer::tokenize(&source).map_err(|e| e.with_file(file.clone()))?;

    parser::parse(&tokens).map_err(|e| e.with_file(file))
}
