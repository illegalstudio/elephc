//! Purpose:
//! Gives a closed world a callable for each builtin a string names when the builtin has no
//! run-time wrapper of its own.
//!
//! Called from:
//! - `crate::pipeline::compile()`, once autoloading has spliced every class file in.
//! - `crate::codegen::lower_inst::callables`, which offers each wrapper under the builtin's name.
//!
//! Key details:
//! - A builtin with a `StaticOnly` or `DirectOnly` policy (`json_encode`, `max`, `implode`) answers
//!   a direct call only. Without an interpreter a name held in a string then ended on
//!   `Call to undefined function <dynamic>()`: twig registers `'json_encode'` as a filter and
//!   `ReflectionCallable` turns it into a closure, which is what `lint:twig` stopped on.
//! - Each wrapper is an ordinary function with the builtin's parameters, taken from its contract,
//!   so a closure over it reflects the same names, count and defaults, and its body is a direct
//!   call. Only the wrappers of names a string literal spells are declared.
//! - Parameters stay untyped: the direct call inside applies the builtin's own coercions.
//! - A builtin taking anything by reference or with a default no literal can spell keeps no
//!   wrapper: the call by name stays the fatal it was.
//! - The helpers are BUILT, never parsed: elephc does not parse PHP source outside tests.

use std::collections::BTreeSet;

use crate::builtins::spec::DefaultSpec;
use crate::errors::CompileError;
use crate::names::Name;
use crate::parser::ast::visit::{walk_nodes, Node};
use crate::parser::ast::{Expr, ExprKind, Program, Stmt, StmtKind};
use crate::span::Span;

/// Inventory group of the wrappers. Forced: lowering reaches them through a name in a string.
pub const GROUP: &str = "builtin_callables";

/// Name fragment of a wrapper, in front of the builtin's name.
const WRAPPER_PREFIX: &str = "__elephc_callable_";

/// Builtins whose checker demands a literal argument, which a wrapper's parameter is not.
const LITERAL_ARGUMENT_BUILTINS: [&str; 6] =
    ["array_chunk", "array_reverse", "array_slice", "class_implements", "class_parents", "class_uses"];

/// The builtin name a compiled function answers for, when it is one of these wrappers.
pub fn builtin_name_of_wrapper(function: &str) -> Option<&str> {
    function.strip_prefix(WRAPPER_PREFIX)
}

/// Declares the wrappers of the builtins this program's string literals name.
pub fn inject_if_used(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Result<Program, CompileError> {
    if !crate::closed_world::is_enabled() {
        return Ok(program);
    }
    let mut names = BTreeSet::new();
    // Each literal's occurrences, and those in a position where the string is never a value a call
    // could receive: a probe's argument (`function_exists('clamp')` asks whether the name exists)
    // and an array key (`$frame['file']`, `array_key_exists('file', $frame)` name an element; the
    // key goes nowhere).
    let mut occurrences: crate::fast_hash::FastMap<&str, usize> = crate::fast_hash::FastMap::default();
    let mut inert: crate::fast_hash::FastMap<&str, usize> = crate::fast_hash::FastMap::default();
    walk_nodes(&program, &mut |node| {
        let inert_position = match node {
            Node::Expr(Expr { kind: ExprKind::StringLiteral(value), .. }) => {
                if value.len() <= 64 {
                    names.insert(value.as_str());
                    *occurrences.entry(value.as_str()).or_default() += 1;
                }
                None
            }
            Node::Expr(Expr { kind: ExprKind::FunctionCall { name, args }, .. })
                if matches!(
                    crate::names::php_symbol_key(name.as_str().trim_start_matches('\\')).as_str(),
                    "function_exists" | "is_callable" | "array_key_exists" | "key_exists"
                ) =>
            {
                args.first()
            }
            Node::Expr(Expr { kind: ExprKind::ArrayAccess { index, .. }, .. }) => Some(&**index),
            Node::Stmt(Stmt {
                kind:
                    StmtKind::ArrayAssign { index, .. }
                    | StmtKind::PropertyArrayAssign { index, .. }
                    | StmtKind::StaticPropertyArrayAssign { index, .. }
                    | StmtKind::StaticPropertyElementRefAssign { index, .. },
                ..
            }) => Some(index),
            Node::Stmt(Stmt { kind: StmtKind::DynamicStaticPropertyWrite { index, .. }, .. }) => {
                index.as_ref()
            }
            _ => None,
        };
        if let Some(value) = literal(inert_position) {
            *inert.entry(value).or_default() += 1;
        }
    });
    names.retain(|name| inert.get(name) != occurrences.get(name));
    // Only a builtin the profile shows: `--strict-php` hides the extension builtins, and a function
    // the program declares under one of their names (php refuses to redeclare a visible one) answers
    // that name itself.
    let strict = crate::strict_php::is_enabled();
    let wrappers: Vec<Stmt> = names
        .iter()
        .filter(|name| {
            crate::types::checker::builtins::is_php_visible_builtin_function_for_profile(name, strict)
        })
        .filter_map(|name| wrapper_of(name))
        .collect();
    if wrappers.is_empty() {
        return Ok(program);
    }
    let mut combined = crate::name_resolver::resolve(wrappers)?;
    inventory.record_program(GROUP, &combined);
    combined.extend(program);
    Ok(combined)
}

/// The string a position holds when it is a literal.
fn literal(position: Option<&Expr>) -> Option<&str> {
    match position {
        Some(Expr { kind: ExprKind::StringLiteral(value), .. }) => Some(value.as_str()),
        _ => None,
    }
}

/// Builds `function __elephc_callable_x($a, $b = 1, ...$rest) { return x($a, $b, ...$rest); }`.
fn wrapper_of(name: &str) -> Option<Stmt> {
    let contract = elephc_builtin_contract::lookup(name)?;
    if crate::codegen_support::callable_dispatch::builtin_may_have_runtime_wrapper(name)
        || LITERAL_ARGUMENT_BUILTINS.contains(&name)
        || contract.name != name
        // A language construct or a dedicated syntax (`buffer_new`, `ptr_get`) is no function a
        // name can call.
        || !matches!(contract.kind, elephc_builtin_contract::BuiltinKind::Function)
        || contract.internal
        || contract.params.iter().any(|param| param.by_ref)
        || contract.variadic.is_some_and(|variadic| {
            !matches!(variadic.passing, elephc_builtin_contract::PassingMode::Value)
        })
    {
        return None;
    }
    let span = Span::dummy();
    let variable = |param: &str| Expr::new(ExprKind::Variable(param.to_string()), span);
    let mut params = Vec::with_capacity(contract.params.len());
    let mut args = Vec::with_capacity(contract.params.len() + 1);
    for param in contract.params {
        let default = match &param.default {
            Some(default) => Some(default_expr(default)?),
            None => None,
        };
        params.push((param.name.to_string(), None, default, false));
        args.push(variable(param.name));
    }
    let variadic = contract.variadic.map(|variadic| variadic.name.to_string());
    if let Some(rest) = &variadic {
        // A spread into a builtin is not lowered. `min` and `max` have an array form that means
        // the same; any other variadic builtin keeps no wrapper.
        if !matches!(name, "min" | "max") {
            return None;
        }
        let first = args.pop()?;
        let call = |target: &str, args: Vec<Expr>| {
            Expr::new(ExprKind::FunctionCall { name: global_name(target), args }, span)
        };
        let all = call(
            "array_merge",
            vec![Expr::new(ExprKind::ArrayLiteral(vec![first.clone()]), span), variable(rest)],
        );
        let no_rest = Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(call("count", vec![variable(rest)])),
                op: crate::parser::ast::BinOp::StrictEq,
                right: Box::new(Expr::new(ExprKind::IntLiteral(0), span)),
            },
            span,
        );
        args.push(Expr::new(
            ExprKind::Ternary {
                condition: Box::new(no_rest),
                then_expr: Box::new(first),
                else_expr: Box::new(all),
            },
            span,
        ));
    }
    let call = Expr::new(
        ExprKind::FunctionCall { name: global_name(name), args },
        span,
    );
    Some(Stmt::new(
        StmtKind::FunctionDecl {
            type_params: Vec::new(),
            name: format!("{WRAPPER_PREFIX}{name}"),
            params,
            param_attributes: Vec::new(),
            variadic,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![Stmt::new(StmtKind::Return(Some(call)), span)],
        },
        span,
    ))
}

/// The fully qualified name of a global function.
fn global_name(name: &str) -> Name {
    Name::from_parts(crate::names::NameKind::FullyQualified, vec![name.to_string()])
}

/// The expression of a contract default, for the forms a literal or constant can spell.
fn default_expr(default: &DefaultSpec) -> Option<Expr> {
    let kind = match default {
        DefaultSpec::Null => ExprKind::Null,
        DefaultSpec::Int(value) => ExprKind::IntLiteral(*value),
        DefaultSpec::Bool(value) => ExprKind::BoolLiteral(*value),
        DefaultSpec::Float(value) => ExprKind::FloatLiteral(*value),
        DefaultSpec::Str(value) => ExprKind::StringLiteral((*value).to_string()),
        DefaultSpec::EmptyArray => ExprKind::ArrayLiteral(Vec::new()),
        DefaultSpec::IntMax => ExprKind::ConstRef(Name::unqualified("PHP_INT_MAX")),
        DefaultSpec::Constant(name) => ExprKind::ConstRef(Name::unqualified(*name)),
        _ => return None,
    };
    Some(Expr::new(kind, Span::dummy()))
}
