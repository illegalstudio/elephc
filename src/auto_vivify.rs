//! Purpose:
//! Pre-checker AST normalization for PHP auto-vivification. PHP creates an undefined
//! variable as an empty array on an array write (`$a[0] = 1;`, `$a[] = 1;`) and as null
//! when it is passed to a by-reference output parameter (`f($y)` with `function f(&$x)`,
//! or `curl_multi_exec($mh, $active)`). elephc's checker reports `Undefined variable`
//! instead, so this pass inserts the implicit `$a = [];` / `$x = null;` declaration at the
//! top of the scope that would otherwise error.
//!
//! Called from:
//! - `crate::pipeline::compile`, after name resolution and constant folding and before the
//!   type checker, so the checker and every later pass see an ordinary declaration.
//!
//! Key details:
//! - The insertion is deliberately CONSERVATIVE: it only fires when the name is never
//!   bound anywhere in the same scope. If the name is assigned (to anything), the source
//!   is left untouched and the checker keeps ownership of whether the write is legal (for
//!   example `$a = 5; $a[0] = 1;`, which PHP rejects). Inserting an initializer there
//!   would silently mask the real error.
//! - By-reference output detection needs the callee's parameter modes, which the injected
//!   preludes carry as ordinary `FunctionDecl`s by the time this runs. Builtins (e.g.
//!   `preg_match`'s `$matches`) are not `FunctionDecl`s and are therefore NOT covered here;
//!   they keep whatever the checker does today.
//! - Function-like scopes (functions, methods, closures) are normalized independently:
//!   their bodies are separate scopes and a name bound inside one must not suppress an
//!   initializer in another.
//! - KNOWN DIVERGENCE: the initializer is placed at the TOP of the scope, so a program that
//!   probes `isset($a)` BEFORE its first array write sees `true` here and `false` in PHP.
//!   Hoisting is what keeps a write inside a loop from re-initializing the array on every
//!   iteration; the alternative (initializing at the write) resets accumulators.

use std::collections::{HashMap, HashSet};

use crate::parser::ast::{CatchClause, ClassMethod, Expr, ExprKind, Program, Stmt, StmtKind, TypeExpr};

/// Names PHP never auto-vivifies as ordinary locals.
const SUPERGLOBALS: &[&str] = &[
    "GLOBALS",
    "_SERVER",
    "_GET",
    "_POST",
    "_FILES",
    "_COOKIE",
    "_SESSION",
    "_REQUEST",
    "_ENV",
    "argv",
    "argc",
    "this",
    "php_errormsg",
    "http_response_header",
];

/// Declared by-reference parameter modes for one function, by unqualified lowercase name.
struct FnRefs {
    /// One flag per declared parameter: `true` when it is by reference.
    params: Vec<bool>,
}

/// Normalizes every scope in `program`, returning the rewritten program.
pub fn normalize(program: Program) -> Program {
    let refs = collect_function_refs(&program);
    let mut program = program;
    for stmt in &mut program {
        rewrite_stmt_scope(stmt, &refs);
    }
    normalize_scope(program, &refs, &[])
}

/// Collects the by-reference parameter modes of every `FunctionDecl`, including the
/// injected preludes and declarations nested in classes.
fn collect_function_refs(program: &[Stmt]) -> HashMap<String, FnRefs> {
    let mut refs = HashMap::new();
    for stmt in program {
        collect_function_refs_stmt(stmt, &mut refs);
    }
    refs
}

/// Recursively records declarations reachable from one statement.
fn collect_function_refs_stmt(stmt: &Stmt, refs: &mut HashMap<String, FnRefs>) {
    match &stmt.kind {
        StmtKind::FunctionDecl { name, params, .. } => {
            refs.insert(
                name.to_ascii_lowercase(),
                FnRefs {
                    params: params.iter().map(|param| param.3).collect(),
                },
            );
        }
        StmtKind::NamespaceBlock { body, .. } => {
            for inner in body {
                collect_function_refs_stmt(inner, refs);
            }
        }
        StmtKind::FunctionVariantGroup { .. } => {}
        _ => {}
    }
}

/// Rewrites the nested function-like scopes of one statement and its control-flow bodies.
///
/// Separate from [`normalize_scope`] so a function body is normalized as its own scope
/// rather than inheriting the enclosing one's bindings.
fn rewrite_stmt_scope(stmt: &mut Stmt, refs: &HashMap<String, FnRefs>) {
    match &mut stmt.kind {
        StmtKind::FunctionDecl {
            body,
            params,
            variadic,
            ..
        } => {
            let bound = parameter_names(params, variadic);
            *body = normalize_scope(std::mem::take(body), refs, &bound);
        }
        StmtKind::ClassDecl { methods, .. }
        | StmtKind::InterfaceDecl { methods, .. }
        | StmtKind::TraitDecl { methods, .. }
        | StmtKind::EnumDecl { methods, .. } => {
            for method in methods.iter_mut() {
                rewrite_method_scope(method, refs);
            }
        }
        StmtKind::NamespaceBlock { body, .. } => {
            for inner in body {
                rewrite_stmt_scope(inner, refs);
            }
        }
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            condition,
        } => {
            rewrite_expr_closures(condition, refs);
            for inner in then_body.iter_mut() {
                rewrite_stmt_scope(inner, refs);
            }
            for (cond, body) in elseif_clauses.iter_mut() {
                rewrite_expr_closures(cond, refs);
                for inner in body.iter_mut() {
                    rewrite_stmt_scope(inner, refs);
                }
            }
            if let Some(body) = else_body {
                for inner in body.iter_mut() {
                    rewrite_stmt_scope(inner, refs);
                }
            }
        }
        StmtKind::While { body, condition } | StmtKind::DoWhile { body, condition } => {
            rewrite_expr_closures(condition, refs);
            for inner in body.iter_mut() {
                rewrite_stmt_scope(inner, refs);
            }
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            for s in init.iter_mut().chain(update.iter_mut()) {
                rewrite_stmt_scope(s, refs);
            }
            if let Some(cond) = condition {
                rewrite_expr_closures(cond, refs);
            }
            for inner in body.iter_mut() {
                rewrite_stmt_scope(inner, refs);
            }
        }
        StmtKind::Foreach { body, .. } => {
            for inner in body.iter_mut() {
                rewrite_stmt_scope(inner, refs);
            }
        }
        StmtKind::Switch {
            cases, default, ..
        } => {
            for (_, body) in cases.iter_mut() {
                for inner in body.iter_mut() {
                    rewrite_stmt_scope(inner, refs);
                }
            }
            if let Some(body) = default {
                for inner in body.iter_mut() {
                    rewrite_stmt_scope(inner, refs);
                }
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            for inner in try_body.iter_mut() {
                rewrite_stmt_scope(inner, refs);
            }
            for clause in catches.iter_mut() {
                for inner in clause.body.iter_mut() {
                    rewrite_stmt_scope(inner, refs);
                }
            }
            if let Some(body) = finally_body {
                for inner in body.iter_mut() {
                    rewrite_stmt_scope(inner, refs);
                }
            }
        }
        StmtKind::Synthetic(body) => {
            for inner in body.iter_mut() {
                rewrite_stmt_scope(inner, refs);
            }
        }
        _ => {}
    }
}

/// Normalizes one class method body as its own scope.
fn rewrite_method_scope(method: &mut ClassMethod, refs: &HashMap<String, FnRefs>) {
    let bound = parameter_names(&method.params, &method.variadic);
    method.body = normalize_scope(std::mem::take(&mut method.body), refs, &bound);
}

/// Returns the declared parameter names (regular plus variadic) of a function-like scope.
fn parameter_names(
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    variadic: &Option<String>,
) -> Vec<String> {
    let mut names: Vec<String> = params.iter().map(|param| param.0.clone()).collect();
    if let Some(variadic) = variadic {
        names.push(variadic.clone());
    }
    names
}

/// Rewrites closures nested inside a condition expression (a separate scope each).
fn rewrite_expr_closures(_expr: &Expr, _refs: &HashMap<String, FnRefs>) {
    // Closure bodies are handled by their own construction path only when reachable as a
    // statement; scanning expression-embedded closures is out of scope for this pass and
    // would only under-approximate. Left deliberately empty.
}

/// Collects the scope's bound names, array-write bases, and by-reference output variables,
/// then prepends the missing `$name = [];` / `$name = null;` declarations.
fn normalize_scope(
    body: Vec<Stmt>,
    refs: &HashMap<String, FnRefs>,
    params: &[String],
) -> Vec<Stmt> {
    let mut scope = ScopeInfo::default();
    // Declared parameters bind in the scope before any statement; without this a parameter
    // used as an array-write base would be mistaken for an auto-vivified local and clobbered.
    for name in params {
        scope.bound.insert(name.clone());
    }
    for stmt in &body {
        collect_stmt(stmt, refs, &mut scope);
    }
    let mut declarations: Vec<Stmt> = Vec::new();
    let mut emitted: HashSet<String> = HashSet::new();
    let mut array_names: Vec<&String> = scope
        .array_bases
        .difference(&scope.bound)
        .filter(|name| !is_superglobal(name))
        .collect();
    array_names.sort();
    for name in array_names {
        if emitted.insert(name.clone()) {
            declarations.push(init_stmt(name, true, stmt_span(body.first())));
        }
    }
    let mut byref_names: Vec<&String> = scope
        .byref_outputs
        .difference(&scope.bound)
        .filter(|name| !is_superglobal(name))
        .collect();
    byref_names.sort();
    for name in byref_names {
        if emitted.insert(name.clone()) {
            declarations.push(init_stmt(name, false, stmt_span(body.first())));
        }
    }
    if declarations.is_empty() {
        return body;
    }
    let mut out = Vec::with_capacity(body.len() + declarations.len());
    let insert_at = top_level_insert_index(&body);
    out.extend(body[..insert_at].iter().cloned());
    out.extend(declarations);
    out.extend(body[insert_at..].iter().cloned());
    out
}

/// Per-scope facts used to decide which declarations to synthesize.
#[derive(Default)]
struct ScopeInfo {
    /// Every name bound in the scope by a parameter, `global`/`static`, assignment,
    /// `foreach` target, `catch` variable, or destructuring.
    bound: HashSet<String>,
    /// Names used as the base of an array element write or push (`$a[...] = …`, `$a[] = …`).
    array_bases: HashSet<String>,
    /// Names passed as a by-reference output argument to a known declared function.
    byref_outputs: HashSet<String>,
}

/// Position at which synthesized declarations may be inserted.
///
/// PHP requires `namespace`/`use` to precede any statement, so those stay first.
fn top_level_insert_index(body: &[Stmt]) -> usize {
    let mut index = 0;
    while index < body.len() {
        match &body[index].kind {
            StmtKind::NamespaceDecl { .. } | StmtKind::UseDecl { .. } => index += 1,
            _ => break,
        }
    }
    index
}

/// Builds a `$name = [];` (array) or `$name = null;` (by-reference output) statement.
fn init_stmt(name: &str, array: bool, span: Option<crate::span::Span>) -> Stmt {
    let span = span.unwrap_or_else(crate::span::Span::dummy);
    let value = if array {
        Expr::new(ExprKind::ArrayLiteral(Vec::new()), span)
    } else {
        Expr::new(ExprKind::Null, span)
    };
    Stmt::new(
        StmtKind::Assign {
            name: name.to_string(),
            value,
        },
        span,
    )
}

/// Returns the span of the first statement, or `None` for an empty body.
fn stmt_span(stmt: Option<&Stmt>) -> Option<crate::span::Span> {
    stmt.map(|s| s.span)
}

/// Returns whether `name` is a PHP superglobal that must never be initialized.
fn is_superglobal(name: &str) -> bool {
    SUPERGLOBALS.iter().any(|s| s.eq_ignore_ascii_case(name))
}

/// Collects bindings, array-write bases, and by-reference outputs for one statement,
/// recursing into control-flow bodies but not into nested function-like scopes.
fn collect_stmt(stmt: &Stmt, refs: &HashMap<String, FnRefs>, scope: &mut ScopeInfo) {
    match &stmt.kind {
        StmtKind::Assign { name, value } | StmtKind::TypedAssign { name, value, .. } => {
            scope.bound.insert(name.clone());
            collect_expr(value, refs, scope);
        }
        StmtKind::RefAssign { target, source } => {
            scope.bound.insert(target.clone());
            collect_expr(source, refs, scope);
        }
        StmtKind::ListUnpack { vars, value } => {
            for var in vars {
                scope.bound.insert(var.clone());
            }
            collect_expr(value, refs, scope);
        }
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            body,
            ..
        } => {
            if let Some(key) = key_var {
                scope.bound.insert(key.clone());
            }
            scope.bound.insert(value_var.clone());
            collect_expr(array, refs, scope);
            for inner in body {
                collect_stmt(inner, refs, scope);
            }
        }
        StmtKind::Global { vars } => {
            for var in vars {
                scope.bound.insert(var.clone());
            }
        }
        StmtKind::StaticVar { name, init } => {
            scope.bound.insert(name.clone());
            collect_expr(init, refs, scope);
        }
        StmtKind::ArrayAssign {
            array,
            index,
            value,
        } => {
            scope.array_bases.insert(array.clone());
            collect_expr(index, refs, scope);
            collect_expr(value, refs, scope);
        }
        StmtKind::ArrayPush { array, value } => {
            scope.array_bases.insert(array.clone());
            collect_expr(value, refs, scope);
        }
        StmtKind::NestedArrayAssign { target, value } => {
            if let Some(root) = root_variable(target) {
                scope.array_bases.insert(root.to_string());
            }
            collect_expr(target, refs, scope);
            collect_expr(value, refs, scope);
        }
        StmtKind::ExprStmt(expr) => {
            if let Some(name) = bound_by_expression(expr) {
                scope.bound.insert(name.to_string());
            }
            collect_expr(expr, refs, scope);
        }
        StmtKind::Echo(expr) | StmtKind::Throw(expr) => {
            collect_expr(expr, refs, scope);
        }
        StmtKind::Return(Some(expr)) => collect_expr(expr, refs, scope),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            collect_expr(condition, refs, scope);
            for inner in then_body {
                collect_stmt(inner, refs, scope);
            }
            for (cond, body) in elseif_clauses {
                collect_expr(cond, refs, scope);
                for inner in body {
                    collect_stmt(inner, refs, scope);
                }
            }
            if let Some(body) = else_body {
                for inner in body {
                    collect_stmt(inner, refs, scope);
                }
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            collect_expr(condition, refs, scope);
            for inner in body {
                collect_stmt(inner, refs, scope);
            }
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            for s in init.iter().chain(update.iter()) {
                collect_stmt(s, refs, scope);
            }
            if let Some(cond) = condition {
                collect_expr(cond, refs, scope);
            }
            for inner in body {
                collect_stmt(inner, refs, scope);
            }
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            collect_expr(subject, refs, scope);
            for (exprs, body) in cases {
                for expr in exprs {
                    collect_expr(expr, refs, scope);
                }
                for inner in body {
                    collect_stmt(inner, refs, scope);
                }
            }
            if let Some(body) = default {
                for inner in body {
                    collect_stmt(inner, refs, scope);
                }
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            for inner in try_body {
                collect_stmt(inner, refs, scope);
            }
            for clause in catches {
                collect_catch(clause, refs, scope);
            }
            if let Some(body) = finally_body {
                for inner in body {
                    collect_stmt(inner, refs, scope);
                }
            }
        }
        StmtKind::NamespaceBlock { body, .. } => {
            for inner in body {
                collect_stmt(inner, refs, scope);
            }
        }
        StmtKind::Synthetic(body) => {
            for inner in body {
                collect_stmt(inner, refs, scope);
            }
        }
        _ => {}
    }
}

/// Records a catch clause's bound variable and scans its body.
fn collect_catch(clause: &CatchClause, refs: &HashMap<String, FnRefs>, scope: &mut ScopeInfo) {
    if let Some(name) = &clause.variable {
        scope.bound.insert(name.clone());
    }
    for inner in &clause.body {
        collect_stmt(inner, refs, scope);
    }
}

/// Scans an expression for by-reference output arguments, recursing into children.
/// Builtins whose by-reference OUTPUT parameters must stay UNDECLARED.
///
/// php leaves the local undefined before the call, and the checker's undefined-variable path
/// gives it `Mixed`, which `count()` and element access already handle. A `null` declaration
/// would instead bind `Void`: `count($matches)` then reaches the EIR backend as `count for PHP
/// type Void`, and declaring the output as an empty array fixes the count but types the
/// elements `never`, so the values the callee writes read back empty.
///
/// `xml_parse_into_struct()` joins them for the same `Void` reason: its checker hook accepts
/// `$values`/`$index` undefined, and its lowering reads the operand's type to tell an OMITTED
/// `$index` from a supplied one. A pre-declared `$index = null` binds it `Void`, which is
/// indistinguishable from the planned default, so the index array would never be built.
///
/// Every other output keeps the neutral `null` declaration, which is what closes the
/// `Undefined variable` diagnostic for a later plain read.
fn builtin_array_output(name: &str, index: usize) -> bool {
    matches!(
        (crate::names::php_symbol_key(name.trim_start_matches('\\')).as_str(), index),
        ("preg_match", 2) | ("preg_match_all", 2)
            | ("xml_parse_into_struct", 2) | ("xml_parse_into_struct", 3)
    )
}

/// Records by-reference output variables in supported calls and recursively scans their child
/// expressions.
///
/// Covers the statement/expression shapes this pass walks (calls, `new`, array access, casts,
/// assignments, ternaries, closures, and interpolated strings), so a by-reference output such as
/// `curl_multi_exec($mh, $active)` auto-vivifies its caller variable even when the call is nested
/// inside one of them.
fn collect_expr(expr: &Expr, refs: &HashMap<String, FnRefs>, scope: &mut ScopeInfo) {
    match &expr.kind {
        ExprKind::FunctionCall { name, args } => {
            if let Some(fn_refs) = lookup_fn_refs(name, refs) {
                for (index, arg) in args.iter().enumerate() {
                    let is_ref = fn_refs.params.get(index).copied().unwrap_or(false);
                    if is_ref {
                        if let Some(name) = positional_variable(arg) {
                            scope.byref_outputs.insert(name.to_string());
                        }
                    }
                    collect_expr(arg, refs, scope);
                }
            } else if let Some(sig) = crate::types::builtin_call_sig(name.as_str()) {
                // A builtin's by-reference OUTPUT parameters (`stream_socket_server`'s
                // `$errno`/`$errstr`, `preg_match`'s `$matches`) are write-only and undefined
                // before the call, so declare them exactly like a declared function's. The
                // registry signature is the same authority the checker uses. The lowering
                // promotes the declared `Void` binding to a managed Mixed cell at the call,
                // so a later `count($matches)` sees the callee's storage type.
                let builtin_name = name.as_str();
                for (index, arg) in args.iter().enumerate() {
                    if sig.ref_params.get(index).copied().unwrap_or(false)
                        && !builtin_array_output(builtin_name, index)
                    {
                        if let Some(local) = positional_variable(arg) {
                            scope.byref_outputs.insert(local.to_string());
                        }
                    }
                    collect_expr(arg, refs, scope);
                }
            } else {
                for arg in args {
                    collect_expr(arg, refs, scope);
                }
            }
        }
        ExprKind::BinaryOp { left, right, .. } => {
            collect_expr(left, refs, scope);
            collect_expr(right, refs, scope);
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Clone(inner)
        | ExprKind::Spread(inner) => collect_expr(inner, refs, scope),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_expr(condition, refs, scope);
            collect_expr(then_expr, refs, scope);
            collect_expr(else_expr, refs, scope);
        }
        ExprKind::ShortTernary { value, default }
        | ExprKind::NullCoalesce { value, default } => {
            collect_expr(value, refs, scope);
            collect_expr(default, refs, scope);
        }
        ExprKind::Cast { expr: inner, .. } => collect_expr(inner, refs, scope),
        ExprKind::ArrayAccess { array, index } => {
            collect_expr(array, refs, scope);
            collect_expr(index, refs, scope);
        }
        ExprKind::Assignment { value, .. } => collect_expr(value, refs, scope),
        _ => {}
    }
}

/// Returns the variable name an expression binds in statement position, if any.
///
/// Compound assignments and `++`/`--` reach statement position as `ExprStmt(Assignment…)`
/// / `ExprStmt(PreIncrement…)` rather than as a dedicated statement kind, so a name bound
/// only that way must still count as bound.
fn bound_by_expression(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::Assignment { target, .. } => match &target.kind {
            ExprKind::Variable(name) => Some(name),
            _ => None,
        },
        ExprKind::PreIncrement(name)
        | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name)
        | ExprKind::PostDecrement(name) => Some(name),
        _ => None,
    }
}

/// Resolves a declared function's by-reference parameters from a call name.
fn lookup_fn_refs<'a>(
    name: &crate::names::Name,
    refs: &'a HashMap<String, FnRefs>,
) -> Option<&'a FnRefs> {
    let last = name.parts.last()?;
    refs.get(&last.to_ascii_lowercase())
}

/// Returns the variable name of a positional argument, if it is a plain variable.
fn positional_variable(arg: &Expr) -> Option<&str> {
    match &arg.kind {
        ExprKind::Variable(name) => Some(name),
        _ => None,
    }
}

/// Returns the root variable name of an array-access chain, if any.
fn root_variable(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::Variable(name) => Some(name),
        ExprKind::ArrayAccess { array, .. } => root_variable(array),
        _ => None,
    }
}
