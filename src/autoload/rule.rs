//! Purpose:
//! Extracts supported user autoload closures from top-level `spl_autoload_register` calls.
//! Converts literal, variable-stored, and named-function loaders into compile-time rules.
//!
//! Called from:
//! - `crate::autoload::registry::Registry::build()`
//!
//! Key details:
//! - Matching unregister calls remove earlier rules from the compile-time discovery chain.
//! - `spl_autoload_*` builtin names are matched case-insensitively before name resolution runs.
//! - Static extraction preserves runtime registration and callback execution.

use std::collections::{HashMap, HashSet};

use crate::errors::CompileWarning;
use crate::names::{canonical_name_for_decl, php_symbol_key};
use crate::parser::ast::{BinOp, Expr, ExprKind, Program, Stmt, StmtKind};

#[derive(Clone, Debug)]
/// Autoload rule extracted from a `spl_autoload_register` call.
///
/// Stores the parameter name that receives the candidate class name
/// (typically "name" or "class") and the closure body to execute symbolically
/// at class-load time.
pub struct AutoloadRule {
    /// Name of the closure parameter that receives the candidate class name
    /// (typically "name" or "class").
    pub param_name: String,
    /// The closure body to execute symbolically.
    pub body: Vec<Stmt>,
}

impl PartialEq for AutoloadRule {
    /// Compares two rules by parameter name and body AST.
    ///
    /// Used by `spl_autoload_unregister` to find and remove a previously
    /// registered rule from the chain.
    fn eq(&self, other: &Self) -> bool {
        self.param_name == other.param_name && self.body == other.body
    }
}

/// Extract compile-time rules from statically analyzable
/// `spl_autoload_register` / `spl_autoload_unregister` calls while preserving
/// the original program for runtime execution.
///
/// Rules are returned in PHP-equivalent chain order: append by default,
/// prepend when the third argument folds to true. `unregister` removes
/// the first chain entry whose body equals the target's body.
pub fn collect_register_calls(
    program: Program,
) -> (Program, Vec<AutoloadRule>, Vec<CompileWarning>) {
    // -- Pass 0: flatten top-level `if` statements whose condition folds
    //    to a literal bool. PHP autoloader code routinely guards
    //    spl_autoload_register with `if (PHP_SAPI === 'cli')` etc.; when
    //    the condition decides at compile time, we want the chosen branch
    //    to participate in collection.
    let program = flatten_foldable_ifs(program);

    let local_functions = collect_declared_function_keys(&program);

    // -- Pass 1: index top-level functions whose bodies look like autoload rules.
    let mut function_rules: HashMap<String, FunctionSource> = HashMap::new();
    for stmt in &program {
        if let StmtKind::FunctionDecl {
            name,
            params,
            variadic,
            body,
            ..
        } = &stmt.kind
        {
            if let Ok(rule) = build_rule(params, variadic.as_deref(), body) {
                function_rules.insert(
                    name.clone(),
                    FunctionSource { rule },
                );
            }
        }
    }

    // -- Pass 2: walk in source order, threading variable bindings.
    let mut chain: Vec<AutoloadRule> = Vec::new();
    let mut current_var_bindings: HashMap<String, VarBinding> = HashMap::new();

    let mut current_namespace: Option<String> = None;
    for stmt in &program {
        if let StmtKind::NamespaceDecl { name } = &stmt.kind {
            current_namespace = name.as_ref().map(|name| name.as_str().to_string());
            continue;
        }
        if classify_and_process(
            stmt,
            current_namespace.as_deref(),
            &current_var_bindings,
            &function_rules,
            &local_functions,
            &mut chain,
        ) {
            continue;
        }
        update_var_bindings(stmt, &mut current_var_bindings);
    }

    // Static rules accelerate compile-time discovery only. PHP still executes the original
    // registration and callback at runtime, including for dynamic class names.
    (program, chain, Vec::new())
}

#[derive(Clone)]
/// Holds an autoload rule extracted from a top-level function declaration
/// along with the statement index of the declaration itself.
struct FunctionSource {
    rule: AutoloadRule,
}

#[derive(Clone)]
/// Tracks a variable binding where the right-hand side is a closure
/// that may be used as an autoload rule.
struct VarBinding {
    rule: AutoloadRule,
}

/// Result of classifying a statically analyzable autoload call.
enum CallKind {
    /// A conforming `spl_autoload_register` call. `prepend` controls chain order.
    Register {
        rule: AutoloadRule,
        prepend: bool,
    },
    /// A `spl_autoload_unregister` call. Removes the first chain entry whose
    /// body equals the rule's body.
    Unregister {
        rule: AutoloadRule,
    },
}

/// Classify a statement as an autoload call and update compile-time discovery state.
fn classify_and_process(
    stmt: &Stmt,
    current_namespace: Option<&str>,
    var_bindings: &HashMap<String, VarBinding>,
    function_rules: &HashMap<String, FunctionSource>,
    local_functions: &HashSet<String>,
    chain: &mut Vec<AutoloadRule>,
) -> bool {
    let Some(call) = classify_call(
        stmt,
        current_namespace,
        var_bindings,
        function_rules,
        local_functions,
    ) else {
        return false;
    };
    match call {
        CallKind::Register {
            rule,
            prepend,
        } => {
            if prepend {
                chain.insert(0, rule);
            } else {
                chain.push(rule);
            }
        }
        CallKind::Unregister { rule } => {
            if let Some(pos) = chain.iter().position(|r| r == &rule) {
                chain.remove(pos);
            }
        }
    }
    true
}

/// Classifies a statement as a `spl_autoload_register` or
/// `spl_autoload_unregister` call. Returns `Some(CallKind)` if it matches,
/// `None` otherwise.
fn classify_call(
    stmt: &Stmt,
    current_namespace: Option<&str>,
    var_bindings: &HashMap<String, VarBinding>,
    function_rules: &HashMap<String, FunctionSource>,
    local_functions: &HashSet<String>,
) -> Option<CallKind> {
    let StmtKind::ExprStmt(expr) = &stmt.kind else {
        return None;
    };
    let ExprKind::FunctionCall { name, args } = &expr.kind else {
        return None;
    };
    if is_builtin_autoload_call(
        name,
        current_namespace,
        local_functions,
        "spl_autoload_register",
    ) {
        let first = args.first()?;
        match resolve_callable(first, var_bindings, function_rules) {
            Resolved::Rule(rule) => {
                let prepend = args.get(2).is_some_and(|arg| literal_bool(&arg.kind));
                Some(CallKind::Register { rule, prepend })
            }
            // Static rule extraction is only an optimization. If the callback needs runtime
            // execution (for example it branches before requiring a file), leave the original
            // registration and closure in the program so the ordinary SPL implementation can
            // preserve PHP's behavior.
            Resolved::StripUnmatched => None,
            Resolved::Unknown => None,
        }
    } else if is_builtin_autoload_call(
        name,
        current_namespace,
        local_functions,
        "spl_autoload_unregister",
    ) {
        let first = args.first()?;
        match resolve_callable(first, var_bindings, function_rules) {
            Resolved::Rule(rule) => Some(CallKind::Unregister { rule }),
            Resolved::StripUnmatched => None,
            Resolved::Unknown => None,
        }
    } else {
        None
    }
}

/// Checks whether `name` refers to the given built-in autoload function.
/// Matching is case-insensitive. Returns `false` if a user-defined function
/// with the same name exists in the current namespace scope.
fn is_builtin_autoload_call(
    name: &crate::names::Name,
    current_namespace: Option<&str>,
    local_functions: &HashSet<String>,
    builtin: &str,
) -> bool {
    if !name.as_str().eq_ignore_ascii_case(builtin) {
        return false;
    }
    if name.is_fully_qualified() {
        return true;
    }
    if !name.is_unqualified() {
        return false;
    }
    let Some(namespace) = current_namespace.filter(|namespace| !namespace.is_empty()) else {
        return true;
    };
    let local_name = canonical_name_for_decl(Some(namespace), builtin);
    !local_functions.contains(&php_symbol_key(&local_name))
}

/// Collects the canonical PHP symbol keys of all top-level function
/// declarations in the program, including those inside namespace blocks.
fn collect_declared_function_keys(program: &[Stmt]) -> HashSet<String> {
    let mut out = HashSet::new();
    collect_declared_function_keys_in(program, None, &mut out);
    out
}

/// Recursively walks `program` statements, updating `current_namespace`
/// on `NamespaceDecl` / `NamespaceBlock` nodes and inserting canonical
/// function keys into `out` for every `FunctionDecl` encountered.
fn collect_declared_function_keys_in(
    program: &[Stmt],
    mut current_namespace: Option<String>,
    out: &mut HashSet<String>,
) {
    for stmt in program {
        match &stmt.kind {
            StmtKind::NamespaceDecl { name } => {
                current_namespace = name.as_ref().map(|name| name.as_str().to_string());
            }
            StmtKind::NamespaceBlock { name, body } => {
                collect_declared_function_keys_in(
                    body,
                    name.as_ref().map(|name| name.as_str().to_string()),
                    out,
                );
            }
            StmtKind::FunctionDecl { name, .. } => {
                let canonical = canonical_name_for_decl(current_namespace.as_deref(), name);
                out.insert(php_symbol_key(&canonical));
            }
            _ => {}
        }
    }
}

/// Result of resolving the first argument of `spl_autoload_register`.
/// Indicates whether a valid rule was extracted, the call should be
/// stripped because the closure didn't conform, or the call should be
/// left for the runtime stub.
enum Resolved {
    /// Successfully resolved to an autoload rule.
    Rule(AutoloadRule),
    /// First arg is a closure literal that cannot be represented as a static rule.
    /// Keep the original call for ordinary runtime SPL behavior.
    StripUnmatched,
    /// First arg is something that cannot be resolved at compile time
    /// (method callable, complex expression, etc.). Leave the call for
    /// the runtime implementation.
    Unknown,
}

/// Resolves the first argument of `spl_autoload_register` to an
/// `AutoloadRule`. Handles closure literals, variable references
/// bound to closures, and string literals naming a function.
/// Returns `Resolved` indicating the outcome.
fn resolve_callable(
    arg: &Expr,
    var_bindings: &HashMap<String, VarBinding>,
    function_rules: &HashMap<String, FunctionSource>,
) -> Resolved {
    match &arg.kind {
        ExprKind::Closure { .. } => match extract_closure_rule(arg) {
            Ok(rule) => Resolved::Rule(rule),
            Err(_) => Resolved::StripUnmatched,
        },
        ExprKind::Variable(name) => match var_bindings.get(name) {
            Some(binding) => Resolved::Rule(binding.rule.clone()),
            None => Resolved::Unknown,
        },
        ExprKind::StringLiteral(func_name) => {
            let cleaned = func_name.trim_start_matches('\\');
            match function_rules.get(cleaned) {
                Some(src) => Resolved::Rule(src.rule.clone()),
                None => Resolved::Unknown,
            }
        }
        _ => Resolved::Unknown,
    }
}

/// Inspects `stmt` and, if it is an assignment of a conforming closure to
/// a variable, updates `bindings` to record the rule. Non-conforming or
/// non-closure assignments remove any earlier binding for that variable.
fn update_var_bindings(stmt: &Stmt, bindings: &mut HashMap<String, VarBinding>) {
    let StmtKind::Assign { name, value } = &stmt.kind else {
        return;
    };
    match extract_closure_rule(value) {
        Ok(rule) => {
            bindings.insert(name.clone(), VarBinding { rule });
        }
        Err(_) => {
            // A non-closure (or non-conforming closure) assignment to this
            // variable invalidates any earlier closure binding.
            bindings.remove(name);
        }
    }
}

/// Extract an AutoloadRule from a closure expression, or an error string on failure.
fn extract_closure_rule(closure_arg: &Expr) -> Result<AutoloadRule, &'static str> {
    let ExprKind::Closure {
        params,
        body,
        captures,
        variadic,
        ..
    } = &closure_arg.kind
    else {
        return Err("closure expected");
    };
    if !captures.is_empty() {
        return Err("`use(...)` captures aren't analysed at compile time");
    }
    build_rule(params, variadic.as_deref(), body)
}

/// Build an AutoloadRule from closure parameters and body if it meets the constraints.
fn build_rule(
    params: &[(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)],
    variadic: Option<&str>,
    body: &[Stmt],
) -> Result<AutoloadRule, &'static str> {
    if variadic.is_some() {
        return Err("variadic closures aren't supported");
    }
    if params.is_empty() {
        return Err("closure must have one parameter: the candidate class name");
    }
    if params.len() > 1 {
        return Err("closure must have exactly one parameter");
    }
    Ok(AutoloadRule {
        param_name: params[0].0.clone(),
        body: body.to_vec(),
    })
}

/// Evaluate a literal boolean expression (bool or non-zero int literal).
fn literal_bool(kind: &ExprKind) -> bool {
    match kind {
        ExprKind::BoolLiteral(b) => *b,
        ExprKind::IntLiteral(n) => *n != 0,
        _ => false,
    }
}

/// Flatten top-level `if` statements whose condition folds to a literal
/// bool. The chosen branch's statements are inlined at the if's position;
/// non-taken branches are dropped. Non-foldable conditions leave the
/// statement unchanged.
fn flatten_foldable_ifs(program: Program) -> Program {
    let mut out: Program = Vec::new();
    for stmt in program {
        match stmt.kind {
            StmtKind::If {
                ref condition,
                ref then_body,
                ref elseif_clauses,
                ref else_body,
            } => match select_taken_branch(condition, then_body, elseif_clauses, else_body) {
                BranchOutcome::Taken(body) => {
                    out.extend(flatten_foldable_ifs(body));
                }
                BranchOutcome::None => {
                    // Condition folded to false and no else branch: drop entirely.
                }
                BranchOutcome::Unfoldable => {
                    out.push(stmt);
                }
            },
            _ => out.push(stmt),
        }
    }
    out
}

enum BranchOutcome {
    /// The condition evaluated to a known boolean at compile time and the
    /// corresponding branch's statements should replace the `if` statement.
    Taken(Vec<Stmt>),
    /// The condition evaluated to `false` and no `else` branch exists.
    None,
    /// The condition could not be evaluated at compile time; the `if`
    /// statement is left unchanged.
    Unfoldable,
}

/// Evaluates the condition of an `if` statement at compile time,
/// returning which branch should be taken, if determinable.
fn select_taken_branch(
    condition: &Expr,
    then_body: &[Stmt],
    elseif_clauses: &[(Expr, Vec<Stmt>)],
    else_body: &Option<Vec<Stmt>>,
) -> BranchOutcome {
    match fold_compile_time_bool(condition) {
        Some(true) => return BranchOutcome::Taken(then_body.to_vec()),
        Some(false) => {}
        None => return BranchOutcome::Unfoldable,
    }
    for (clause_cond, clause_body) in elseif_clauses {
        match fold_compile_time_bool(clause_cond) {
            Some(true) => return BranchOutcome::Taken(clause_body.to_vec()),
            Some(false) => continue,
            None => return BranchOutcome::Unfoldable,
        }
    }
    match else_body {
        Some(body) => BranchOutcome::Taken(body.clone()),
        None => BranchOutcome::None,
    }
}

/// Best-effort compile-time boolean fold. Returns `None` for any
/// expression we can't decide. Deliberately narrow: we only need to
/// decide the conditions a typical autoloader guard uses.
fn fold_compile_time_bool(expr: &Expr) -> Option<bool> {
    match &expr.kind {
        ExprKind::BoolLiteral(b) => Some(*b),
        ExprKind::IntLiteral(n) => Some(*n != 0),
        ExprKind::Null => Some(false),
        ExprKind::Not(inner) => fold_compile_time_bool(inner).map(|b| !b),
        ExprKind::BinaryOp { left, op, right } => match op {
            BinOp::And => match fold_compile_time_bool(left)? {
                false => Some(false),
                true => fold_compile_time_bool(right),
            },
            BinOp::Or => match fold_compile_time_bool(left)? {
                true => Some(true),
                false => fold_compile_time_bool(right),
            },
            BinOp::Eq | BinOp::StrictEq => {
                let l = fold_compile_time_value(left)?;
                let r = fold_compile_time_value(right)?;
                Some(l == r)
            }
            BinOp::NotEq | BinOp::StrictNotEq => {
                let l = fold_compile_time_value(left)?;
                let r = fold_compile_time_value(right)?;
                Some(l != r)
            }
            _ => None,
        },
        _ => None,
    }
}

/// Represents a literal value that can be determined at compile time.
/// Used by `fold_compile_time_value` to compare operands of binary expressions.
#[derive(PartialEq, Eq)]
enum FoldedValue {
    Str(String),
    Int(i64),
    Bool(bool),
    Null,
}

/// Attempts to evaluate `expr` to a `FoldedValue` at compile time.
/// Handles string, integer, boolean, and null literals only.
fn fold_compile_time_value(expr: &Expr) -> Option<FoldedValue> {
    match &expr.kind {
        ExprKind::StringLiteral(s) => Some(FoldedValue::Str(s.clone())),
        ExprKind::IntLiteral(n) => Some(FoldedValue::Int(*n)),
        ExprKind::BoolLiteral(b) => Some(FoldedValue::Bool(*b)),
        ExprKind::Null => Some(FoldedValue::Null),
        _ => None,
    }
}
