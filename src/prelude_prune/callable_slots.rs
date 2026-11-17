//! Purpose:
//! Finds the string literals a program hands to declared `callable` parameters of its own
//! functions and methods, or returns from a `callable` return type, before name resolution.
//!
//! Called from:
//! - `crate::prelude_prune::usage::collect`, which records the literals in
//!   `Usage::callable_slot_literals`.
//! - `crate::gmp_prelude::program_uses_gmp` and `crate::xml_prelude::program_uses_surface`,
//!   through that set, so a literal such as `apply("gmp_abs")` injects the prelude its wrapper
//!   calls, as the call `gmp_abs(...)` would.
//!
//! Key details:
//! - Builtin callback slots are recognized elsewhere from the builtin catalog; this covers the
//!   user declarations the catalog cannot know.
//! - Callees are keyed by their last name segment and methods by name across every class,
//!   because this runs before name resolution. That over-approximates only for calls that pass
//!   a builtin's exact name as a string to a parameter declared `callable` somewhere under the
//!   same name, which is never ordinary text in practice.
//! - Only callees named before types count: a call through a variable (`$f("x")`) may pass
//!   ordinary text, so it injects nothing; such a prelude-backed name stops with the runtime-only
//!   builtin error and must be named directly or as `gmp_abs(...)`.
//! - A spread contributes nothing: the parameter each element binds is only known at run time,
//!   and the checker never links a prelude-backed wrapper for such a name.

use std::collections::{HashMap, HashSet};

use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind, TypeExpr};

/// One callee's declared `callable` parameters.
#[derive(Clone, Debug, Default)]
struct Slots {
    /// Positions of regular parameters declared `callable` (alone or in a union).
    positions: HashSet<usize>,
    /// Names of those parameters, for named arguments.
    names: HashSet<String>,
    /// Every regular parameter name, so a named argument not among them reaches the variadic.
    params: HashSet<String>,
    /// The first variadic position when the variadic element is declared `callable`.
    variadic_from: Option<usize>,
}

impl Slots {
    /// Merges another declaration of the same name (methods share a name across classes).
    fn merge(&mut self, other: Slots) {
        self.positions.extend(other.positions);
        self.names.extend(other.names);
        self.params.extend(other.params);
        self.variadic_from = match (self.variadic_from, other.variadic_from) {
            (Some(left), Some(right)) => Some(left.min(right)),
            (left, right) => left.or(right),
        };
    }

    /// Returns whether a positional argument at `index` binds a callable parameter.
    fn positional(&self, index: usize) -> bool {
        self.positions.contains(&index) || self.variadic_from.is_some_and(|from| index >= from)
    }

    /// Returns whether a named argument binds a callable parameter (or a callable variadic).
    fn named(&self, name: &str) -> bool {
        self.names.contains(name) || (self.variadic_from.is_some() && !self.params.contains(name))
    }
}

/// The callable parameter table of one program, by callee key.
#[derive(Clone, Debug, Default)]
pub(crate) struct UserCallableSlots {
    functions: HashMap<String, Slots>,
    methods: HashMap<String, Slots>,
}

impl UserCallableSlots {
    /// Collects every function and method declaration with a parameter declared `callable`.
    pub(crate) fn from_program(program: &[Stmt]) -> Self {
        let mut slots = Self::default();
        slots.collect(program);
        slots
    }

    /// Returns whether the program declares no callable parameter at all.
    pub(crate) fn is_empty(&self) -> bool {
        self.functions.is_empty() && self.methods.is_empty()
    }

    /// Walks the declarations a program can hold at statement level.
    fn collect(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::FunctionDecl { name, params, variadic, variadic_type, .. } => {
                    if let Some(slots) = slots_for(params, variadic.is_some(), variadic_type.as_ref()) {
                        let key = callee_key(name);
                        self.functions.entry(key).or_default().merge(slots);
                    }
                }
                StmtKind::ClassDecl { methods, .. }
                | StmtKind::EnumDecl { methods, .. }
                | StmtKind::InterfaceDecl { methods, .. }
                | StmtKind::TraitDecl { methods, .. } => {
                    for method in methods {
                        if let Some(slots) = slots_for(
                            &method.params, method.variadic.is_some(), method.variadic_type.as_ref(),
                        ) {
                            self.methods.entry(php_symbol_key(&method.name)).or_default().merge(slots);
                        }
                    }
                }
                StmtKind::NamespaceBlock { body, .. }
                | StmtKind::Synthetic(body)
                | StmtKind::IncludeOnceGuard { body, .. } => self.collect(body),
                _ => {}
            }
        }
    }

    /// Records the literals one call hands to callable parameters of a user function.
    pub(crate) fn function_call(&self, name: &str, args: &[Expr], out: &mut HashSet<String>) {
        if let Some(slots) = self.functions.get(&callee_key(name)) {
            record_arguments(slots, args, out);
        }
    }

    /// Records the literals one call hands to callable parameters of a user method; `new`
    /// reaches `__construct`.
    pub(crate) fn method_call(&self, method: &str, args: &[Expr], out: &mut HashSet<String>) {
        if let Some(slots) = self.methods.get(&php_symbol_key(method)) {
            record_arguments(slots, args, out);
        }
    }
}

/// Builds the slots of one parameter list, or `None` when nothing in it is callable.
fn slots_for(
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    has_variadic: bool,
    variadic_type: Option<&TypeExpr>,
) -> Option<Slots> {
    let mut slots = Slots::default();
    for (index, (name, hint, _, _)) in params.iter().enumerate() {
        slots.params.insert(name.clone());
        if hint.as_ref().is_some_and(TypeExpr::declares_callable) {
            slots.positions.insert(index);
            slots.names.insert(name.clone());
        }
    }
    if has_variadic && variadic_type.is_some_and(TypeExpr::declares_callable) {
        slots.variadic_from = Some(params.len());
    }
    (!slots.positions.is_empty() || slots.variadic_from.is_some()).then_some(slots)
}

/// Keys a function by its last name segment, case-insensitively, as an unresolved call names it.
fn callee_key(name: &str) -> String {
    php_symbol_key(name.trim_start_matches('\\').rsplit('\\').next().unwrap_or(name))
}

/// Records the literal names a call's arguments hand to the callee's callable parameters.
fn record_arguments(slots: &Slots, args: &[Expr], out: &mut HashSet<String>) {
    for (index, arg) in args.iter().enumerate() {
        match &arg.kind {
            ExprKind::NamedArg { name, value } if slots.named(name) => record_literal(value, out),
            ExprKind::NamedArg { .. } => {}
            // A spread element binds a parameter only at run time; the checker records such a
            // name only when its wrapper needs no prelude, so none is injected for it here.
            ExprKind::Spread(_) => {}
            _ if slots.positional(index) => record_literal(arg, out),
            _ => {}
        }
    }
}

/// Records one string literal naming a function (a `Class::method` string names no function),
/// through the arms of a conditional, which are the value itself.
///
/// A callable string is fully qualified, so only one leading `\` is dropped: a namespaced name
/// such as `"Vendor\\xml_parser_create"` names a user function, never the global builtin.
pub(crate) fn record_literal(expr: &Expr, out: &mut HashSet<String>) {
    match &expr.kind {
        ExprKind::Ternary { then_expr, else_expr, .. } => {
            record_literal(then_expr, out);
            record_literal(else_expr, out);
        }
        ExprKind::ShortTernary { value, default } | ExprKind::NullCoalesce { value, default } => {
            record_literal(value, out);
            record_literal(default, out);
        }
        ExprKind::Match { arms, default, .. } => {
            for (_, result) in arms {
                record_literal(result, out);
            }
            if let Some(default) = default {
                record_literal(default, out);
            }
        }
        ExprKind::StringLiteral(_) | ExprKind::BinaryOp { .. } => {
            // A concatenation of literals folds to one name before type checking sees it.
            let Some(text) = super::usage::folded_concat_literal(expr) else {
                return;
            };
            let Some(name) = crate::names::literal_symbol_name(&text) else {
                return;
            };
            let name = name.strip_prefix('\\').unwrap_or(&name);
            if !name.is_empty() && !name.contains("::") {
                out.insert(php_symbol_key(name));
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Collects the callable-slot literals of one source fixture.
    fn literals(source: &str) -> HashSet<String> {
        let tokens = crate::lexer::tokenize(source).expect("fixture must tokenize");
        let program = crate::parser::parse(&tokens).expect("fixture must parse");
        crate::prelude_prune::usage::collect(&program).callable_slot_literals
    }

    /// Verifies literals bound to user `callable` parameters, named arguments, callable
    /// variadics and `callable` returns (through conditional arms) are collected, and that
    /// data arguments and spreads are not.
    #[test]
    fn collects_literals_bound_to_user_callable_positions() {
        let found = literals(
            r#"<?php
function apply(string $label, callable $f, callable ...$rest) {}
function pick(int $n): callable { return $n > 0 ? "gmp_abs" : match ($n) { 0 => "gmp_neg", default => "strlen" }; }
class Box { public function run(?callable $f) {} }
apply("gmp_add", "xml_parse_into_struct", "ucfirst", extra: "lcfirst");
apply(f: "md5", label: "trim");
(new Box())->run("strrev");
apply(...["bin2hex", "sha1"]);
"#,
        );
        for name in ["xml_parse_into_struct", "ucfirst", "lcfirst", "md5", "strrev", "gmp_abs", "gmp_neg", "strlen"] {
            assert!(found.contains(name), "missing {name} in {found:?}");
        }
        for name in ["gmp_add", "trim", "bin2hex", "sha1"] {
            assert!(!found.contains(name), "unexpected {name} in {found:?}");
        }
    }

    /// Verifies literals passed through syntactically named value calls (a first-class callable
    /// in a pipe or a call, a fresh object's `__invoke`) and literal concatenations count, while a
    /// call through a variable never does, even beside a closure that declares `callable`.
    #[test]
    fn records_only_named_value_call_targets() {
        let found = literals(
            r#"<?php
class Box { public function __invoke(string $label, callable $fn) {} }
function apply(callable $f) {}
$piped = "gmp_neg" |> apply(...);
(apply(...))("gmp_sqrt");
(new Box())("gmp_add", "gmp_abs");
apply("gmp_" . "sub");
$accept = function (callable $f) {};
$text = function (string $s) { return $s; };
echo $text("xml_parser_create");
$accept("gmp_and");
"#,
        );
        for name in ["gmp_neg", "gmp_sqrt", "gmp_abs", "gmp_sub"] {
            assert!(found.contains(name), "missing {name} in {found:?}");
        }
        for name in ["gmp_add", "xml_parser_create", "gmp_and"] {
            assert!(!found.contains(name), "unexpected {name} in {found:?}");
        }
    }

    /// Verifies a namespaced callback string keeps its namespace, so a user function named like
    /// a builtin (`Vendor\xml_parser_create`) never reads as the global builtin.
    #[test]
    fn keeps_the_namespace_of_a_callable_literal() {
        let found = literals(
            r#"<?php
function apply(callable $f) {}
apply("Vendor\\xml_parser_create");
apply("\\gmp_abs");
"#,
        );
        assert!(found.contains("vendor\\xml_parser_create"), "{found:?}");
        assert!(!found.contains("xml_parser_create"), "{found:?}");
        assert!(found.contains("gmp_abs"), "{found:?}");
    }
}
