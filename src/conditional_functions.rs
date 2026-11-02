//! Purpose:
//! Binds function declarations that PHP binds at EXECUTION rather than at compile time.
//!
//! Called from:
//! - `crate::pipeline::compile()` between constant folding and type checking.
//!
//! Key details:
//! - PHP binds an unconditional top-level `function f() {}` when the file is compiled, but a
//!   declaration nested in a conditional statement (`if`, loop, `switch`, `try`) only when that
//!   statement executes. `function_exists('f')` is false before the branch runs and true after,
//!   the same name may be declared in two mutually exclusive branches, and two separate files may
//!   each declare it behind their own `function_exists()` guard with only the first taking effect.
//! - AOT has one existing mechanism with exactly that shape: the include-loaded function variant
//!   group (`crate::resolver::function_variants`). The public name becomes a dispatcher thunk over
//!   a runtime "active variant" pointer; each declaration compiles under its own private symbol and
//!   a `FunctionVariantMark` stores its address where the declaration used to sit. This pass routes
//!   conditional declarations into that same mechanism instead of inventing a second one.
//! - A conditional declaration of a name this build already provides as a builtin is left exactly
//!   where it is: the checker refuses `Cannot redeclare built-in function` for a variant group, and
//!   such a guard (`if (!function_exists('strlen'))`) is dead in a build that has the builtin.

use std::collections::{BTreeMap, HashSet};

use crate::codegen_support::platform::Target;
use crate::names::php_symbol_key;
use crate::parser::ast::{Program, Stmt, StmtKind};
use crate::source::SourceMode;

/// Rewrites every conditionally executed top-level function declaration into a variant group.
///
/// Returns the program with (a) one hoisted `FunctionDecl` per conditional declaration, renamed to
/// a private variant symbol, (b) one `FunctionVariantGroup` per public name, and (c) a
/// `FunctionVariantMark` left at each declaration's original position so the binding happens when
/// that statement executes, exactly as in PHP.
pub fn bind_conditional_declarations(program: Program, target: Target) -> Program {
    let mut bound = HashSet::new();
    collect_unconditional_bindings(&program, false, &mut bound);

    let mut rewriter = Rewriter {
        bound,
        target,
        hoisted: Vec::new(),
        groups: BTreeMap::new(),
        ordinal: 0,
        dead: false,
    };
    let mut program = rewriter.rewrite_stmts(program, false);
    if rewriter.hoisted.is_empty() {
        return program;
    }

    let mut prefix = Vec::with_capacity(rewriter.hoisted.len() + rewriter.groups.len());
    for (name, variants) in rewriter.groups {
        prefix.push(Stmt::new(
            StmtKind::FunctionVariantGroup { name, variants },
            crate::span::Span::dummy(),
        ));
    }
    prefix.append(&mut rewriter.hoisted);
    prefix.append(&mut program);
    prefix
}

/// Binds, at program start, the functions an `spl_autoload_register` callback's `require` declares.
///
/// The resolver expands `require __DIR__.'/Ext.php'` inside the callback: the class is declared
/// up front (class tables are static), but a function declared in that file becomes a
/// `FunctionVariantMark` that stays INSIDE the callback. The compiled program never has to run the
/// callback for the class, because it is already declared, so the mark never executed and the
/// function was undefined for every caller (`Call to undefined function`) — Symfony's
/// `FormExtension.php` declares `twig_is_root_form()` after its class this way.
///
/// Copying the mark to the program prefix gives the function the same lifetime the class has.
/// Only a name with exactly ONE variant is hoisted: with several, which include branch ran decides
/// the live one and that decision cannot be made without running the callback.
pub fn hoist_autoload_callback_function_marks(mut program: Program) -> Program {
    fn count_variants(stmts: &[Stmt], counts: &mut std::collections::HashMap<String, usize>) {
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::FunctionVariantGroup { name, variants } => {
                    *counts.entry(php_symbol_key(name)).or_default() += variants.len();
                }
                _ => {
                    for (body, _) in child_bodies(&stmt.kind, false) {
                        count_variants(body, counts);
                    }
                }
            }
        }
    }
    fn callback_marks(stmts: &[Stmt], out: &mut Vec<(String, String, crate::span::Span)>) {
        for stmt in stmts {
            if let StmtKind::ExprStmt(expr) = &stmt.kind {
                if let crate::parser::ast::ExprKind::FunctionCall { name, args } = &expr.kind {
                    let called = name.as_str().trim_start_matches('\\').to_ascii_lowercase();
                    if called == "spl_autoload_register" {
                        for arg in args {
                            if let crate::parser::ast::ExprKind::Closure { body, .. } = &arg.kind {
                                closure_marks(body, out);
                            }
                        }
                    }
                }
                continue;
            }
            for (body, _) in child_bodies(&stmt.kind, false) {
                callback_marks(body, out);
            }
        }
    }
    fn closure_marks(stmts: &[Stmt], out: &mut Vec<(String, String, crate::span::Span)>) {
        for stmt in stmts {
            if let StmtKind::FunctionVariantMark { name, variant } = &stmt.kind {
                out.push((name.clone(), variant.clone(), stmt.span));
                continue;
            }
            for (body, _) in child_bodies(&stmt.kind, true) {
                closure_marks(body, out);
            }
        }
    }

    let mut found = Vec::new();
    callback_marks(&program, &mut found);
    if found.is_empty() {
        return program;
    }
    let mut counts = std::collections::HashMap::new();
    count_variants(&program, &mut counts);
    let mut seen = HashSet::new();
    let mut prefix: Vec<Stmt> = Vec::new();
    for (name, variant, span) in found {
        let key = php_symbol_key(&name);
        if counts.get(&key).copied() == Some(1) && seen.insert(key) {
            prefix.push(Stmt::new(StmtKind::FunctionVariantMark { name, variant }, span));
        }
    }
    if prefix.is_empty() {
        return program;
    }
    prefix.append(&mut program);
    prefix
}

/// Collects the PHP symbol keys already bound at compile time, so this pass never competes with one.
///
/// An unconditional top-level declaration and an existing include-variant group both already own
/// their public name; a conditional declaration of that same name keeps today's handling.
fn collect_unconditional_bindings(stmts: &[Stmt], conditional: bool, bound: &mut HashSet<String>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::FunctionDecl { name, .. } if !conditional => {
                bound.insert(php_symbol_key(name));
            }
            StmtKind::FunctionVariantGroup { name, .. } => {
                bound.insert(php_symbol_key(name));
            }
            StmtKind::FunctionVariantMark { name, .. } => {
                bound.insert(php_symbol_key(name));
            }
            _ => {
                for (body, body_conditional) in child_bodies(&stmt.kind, conditional) {
                    collect_unconditional_bindings(body, body_conditional, bound);
                }
            }
        }
    }
}

/// Borrows every statement list nested directly in `kind`, paired with whether it executes
/// conditionally.
///
/// `Synthetic`, `NamespaceBlock` and `IncludeOnceGuard` are transparent wrappers: a declaration
/// directly inside one still binds when its file is compiled. Every other body listed here is
/// entered only when control reaches it. Function and class bodies are deliberately absent — a
/// `function` nested in another function body is a separate PHP feature with its own handling.
fn child_bodies<'a>(kind: &'a StmtKind, conditional: bool) -> Vec<(&'a Vec<Stmt>, bool)> {
    match kind {
        StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => vec![(body, conditional)],
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            let mut bodies = vec![(then_body, true)];
            bodies.extend(elseif_clauses.iter().map(|(_, body)| (body, true)));
            bodies.extend(else_body.iter().map(|body| (body, true)));
            bodies
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            let mut bodies = vec![(then_body, true)];
            bodies.extend(else_body.iter().map(|body| (body, true)));
            bodies
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::Foreach { body, .. } => vec![(body, true)],
        StmtKind::Switch { cases, default, .. } => {
            let mut bodies = cases
                .iter()
                .map(|(_, body)| (body, true))
                .collect::<Vec<_>>();
            bodies.extend(default.iter().map(|body| (body, true)));
            bodies
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            let mut bodies = vec![(try_body, true)];
            bodies.extend(catches.iter().map(|catch| (&catch.body, true)));
            bodies.extend(finally_body.iter().map(|body| (body, true)));
            bodies
        }
        _ => Vec::new(),
    }
}

/// Carries the state of one rewrite over a whole program.
struct Rewriter {
    /// Symbol keys already bound at compile time, which this pass leaves alone.
    bound: HashSet<String>,
    /// The build target, which decides whether a name is a builtin here.
    target: Target,
    /// Renamed declarations, to be emitted once at program top level.
    hoisted: Vec<Stmt>,
    /// Public name to its variant symbols, in declaration order.
    groups: BTreeMap<String, Vec<String>>,
    /// Monotonic counter making each generated variant symbol unique.
    ordinal: usize,
    /// Whether the statements being rewritten follow one that always leaves their list.
    ///
    /// A conditional declaration there never executes, so it is never bound and must not join
    /// a variant group. Symfony's grapheme polyfill opens each bootstrap file with
    /// `if (\PHP_VERSION_ID >= 80500) { return require __DIR__.'/bootstrap85.php'; }` (an
    /// included file's `return` reaches this pass as a `break` out of its `do {} while (false)`
    /// wrapper) and declares the older, differently-typed `grapheme_*` shapes below it, which
    /// would otherwise be refused as signature-mismatched variants of the live ones.
    dead: bool,
}

impl Rewriter {
    /// Rewrites one statement list, recursing into nested bodies.
    fn rewrite_stmts(&mut self, stmts: Vec<Stmt>, conditional: bool) -> Vec<Stmt> {
        let outer = self.dead;
        let mut out = Vec::with_capacity(stmts.len());
        for stmt in stmts {
            let terminates = always_leaves(&stmt);
            out.push(self.rewrite_stmt(stmt, conditional));
            self.dead |= terminates;
        }
        self.dead = outer;
        out
    }

    /// Rewrites one statement, converting a conditional declaration into a mark.
    fn rewrite_stmt(&mut self, mut stmt: Stmt, conditional: bool) -> Stmt {
        if conditional && self.dead && matches!(stmt.kind, StmtKind::FunctionDecl { .. }) {
            return Stmt::new(StmtKind::Synthetic(Vec::new()), stmt.span);
        }
        if conditional {
            if let StmtKind::FunctionDecl { name, .. } = &stmt.kind {
                if let Some(variant) = self.variant_name_for(name, stmt.source_mode) {
                    let public_name = name.clone();
                    let span = stmt.span;
                    if let StmtKind::FunctionDecl { name, .. } = &mut stmt.kind {
                        *name = variant.clone();
                    }
                    self.groups
                        .entry(public_name.clone())
                        .or_default()
                        .push(variant.clone());
                    self.hoisted.push(stmt);
                    return Stmt::new(
                        StmtKind::FunctionVariantMark {
                            name: public_name,
                            variant,
                        },
                        span,
                    );
                }
                return stmt;
            }
        }
        let span = stmt.span;
        let source_mode = stmt.source_mode;
        let strict_types = stmt.strict_types;
        let attributes = std::mem::take(&mut stmt.attributes);
        let kind = self.rewrite_kind(stmt.kind, conditional);
        let mut stmt = Stmt::with_attributes(kind, span, attributes);
        stmt.source_mode = source_mode;
        stmt.strict_types = strict_types;
        stmt
    }

    /// Rewrites the nested bodies of one statement kind, leaving every other kind untouched.
    fn rewrite_kind(&mut self, kind: StmtKind, conditional: bool) -> StmtKind {
        match kind {
            StmtKind::Synthetic(body) => StmtKind::Synthetic(self.rewrite_stmts(body, conditional)),
            StmtKind::NamespaceBlock { name, body } => StmtKind::NamespaceBlock {
                name,
                body: self.rewrite_stmts(body, conditional),
            },
            StmtKind::IncludeOnceGuard { source_path, required, body } => StmtKind::IncludeOnceGuard {
                source_path,
                required,
                body: self.rewrite_stmts(body, conditional),
            },
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => StmtKind::If {
                condition,
                then_body: self.rewrite_stmts(then_body, true),
                elseif_clauses: elseif_clauses
                    .into_iter()
                    .map(|(condition, body)| (condition, self.rewrite_stmts(body, true)))
                    .collect(),
                else_body: else_body.map(|body| self.rewrite_stmts(body, true)),
            },
            StmtKind::IfDef {
                symbol,
                then_body,
                else_body,
            } => StmtKind::IfDef {
                symbol,
                then_body: self.rewrite_stmts(then_body, true),
                else_body: else_body.map(|body| self.rewrite_stmts(body, true)),
            },
            StmtKind::While { condition, body } => StmtKind::While {
                condition,
                body: self.rewrite_stmts(body, true),
            },
            StmtKind::DoWhile { body, condition } => StmtKind::DoWhile {
                body: self.rewrite_stmts(body, true),
                condition,
            },
            StmtKind::For {
                init,
                condition,
                update,
                body,
            } => StmtKind::For {
                init,
                condition,
                update,
                body: self.rewrite_stmts(body, true),
            },
            StmtKind::Foreach {
                array,
                key_var,
                value_var,
                value_by_ref,
                body,
            } => StmtKind::Foreach {
                array,
                key_var,
                value_var,
                value_by_ref,
                body: self.rewrite_stmts(body, true),
            },
            StmtKind::Switch {
                subject,
                cases,
                default,
            } => StmtKind::Switch {
                subject,
                cases: cases
                    .into_iter()
                    .map(|(labels, body)| (labels, self.rewrite_stmts(body, true)))
                    .collect(),
                default: default.map(|body| self.rewrite_stmts(body, true)),
            },
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => StmtKind::Try {
                try_body: self.rewrite_stmts(try_body, true),
                catches: catches
                    .into_iter()
                    .map(|mut catch| {
                        catch.body = self.rewrite_stmts(catch.body, true);
                        catch
                    })
                    .collect(),
                finally_body: finally_body.map(|body| self.rewrite_stmts(body, true)),
            },
            other => other,
        }
    }

    /// Returns the private variant symbol to give one conditional declaration, or `None` to leave it.
    fn variant_name_for(&mut self, public_name: &str, source_mode: SourceMode) -> Option<String> {
        let key = php_symbol_key(public_name);
        if self.bound.contains(&key) {
            return None;
        }
        if declares_a_builtin_of_this_build(public_name, source_mode, self.target) {
            return None;
        }
        let ordinal = self.ordinal;
        self.ordinal += 1;
        let (namespace, local) = split_namespace(public_name);
        let symbol = format!(
            "__elephc_conditional_fn_{}_{}",
            stable_hash_hex(&[&key, &ordinal.to_string()]),
            sanitize_identifier_segment(local)
        );
        Some(match namespace {
            Some(namespace) => format!("{}\\{}", namespace, symbol),
            None => symbol,
        })
    }
}

/// Returns whether this build already provides `name` as a builtin function.
///
/// Built from the same catalog lookup `Checker::collect_function_decls` uses to refuse a
/// redeclaration, so this pass never hands the checker a variant group it would reject. It is
/// deliberately stricter than that check in one way: the checker exempts compiler-generated
/// (`Internal`) source, but a group over a builtin name would also put a user symbol in front of
/// the builtin at every call site, so no builtin name is ever converted whatever declared it.
fn declares_a_builtin_of_this_build(
    name: &str,
    source_mode: SourceMode,
    target: Target,
) -> bool {
    crate::strict_php::with_source_mode(source_mode, || {
        crate::types::checker::builtins::catalog::canonical_builtin_function_name(name)
            .is_some_and(|builtin| {
                crate::types::checker::builtins::catalog::builtin_is_available_for_target(
                    &builtin, target,
                )
            })
    })
}

/// Splits a canonical function name into its namespace and its local part.
/// Whether `stmt` always leaves the statement list it sits in: a `return`, `throw`, `break` or
/// `continue`, or an `if` that constant folding made literally `if (true)` and whose body always
/// leaves.
fn always_leaves(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Return(_) | StmtKind::Throw(_) | StmtKind::Break(_) | StmtKind::Continue(_) => {
            true
        }
        StmtKind::If {
            condition,
            then_body,
            ..
        } => {
            matches!(condition.kind, crate::parser::ast::ExprKind::BoolLiteral(true))
                && then_body.last().is_some_and(always_leaves)
        }
        StmtKind::Synthetic(body) => body.last().is_some_and(always_leaves),
        _ => false,
    }
}

fn split_namespace(name: &str) -> (Option<&str>, &str) {
    let name = name.trim_start_matches('\\');
    match name.rsplit_once('\\') {
        Some((namespace, local)) => (Some(namespace), local),
        None => (None, name),
    }
}

/// Keeps only the characters legal in a generated symbol, so a namespaced or odd name stays linkable.
fn sanitize_identifier_segment(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push_str("fn");
    }
    out
}

/// Hashes the parts into a stable 16-digit hex string, so a rebuild emits the same symbols.
fn stable_hash_hex(parts: &[&str]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for part in parts {
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Span;

    fn function_decl(name: &str) -> Stmt {
        Stmt::new(
            StmtKind::FunctionDecl {
                type_params: Vec::new(),
                name: name.to_string(),
                params: Vec::new(),
                param_attributes: Vec::new(),
                variadic: None,
                variadic_by_ref: false,
                variadic_type: None,
                return_type: None,
                by_ref_return: false,
                body: Vec::new(),
            },
            Span::dummy(),
        )
    }

    fn guarded(body: Vec<Stmt>) -> Stmt {
        Stmt::new(
            StmtKind::If {
                condition: crate::parser::ast::Expr::new(
                    crate::parser::ast::ExprKind::BoolLiteral(true),
                    Span::dummy(),
                ),
                then_body: body,
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            Span::dummy(),
        )
    }

    fn group_names(program: &Program) -> Vec<(String, Vec<String>)> {
        program
            .iter()
            .filter_map(|stmt| match &stmt.kind {
                StmtKind::FunctionVariantGroup { name, variants } => {
                    Some((name.clone(), variants.clone()))
                }
                _ => None,
            })
            .collect()
    }

    fn marks(stmts: &[Stmt]) -> Vec<(String, String)> {
        let mut found = Vec::new();
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::FunctionVariantMark { name, variant } => {
                    found.push((name.clone(), variant.clone()));
                }
                _ => {
                    for (body, _) in child_bodies(&stmt.kind, false) {
                        found.extend(marks(body));
                    }
                }
            }
        }
        found
    }

    #[test]
    fn conditional_declaration_becomes_a_variant_group_with_a_mark_in_place() {
        let program = vec![guarded(vec![function_decl("cond_fn")])];
        let out = bind_conditional_declarations(program, Target::detect_host());
        let groups = group_names(&out);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].0, "cond_fn");
        assert_eq!(groups[0].1.len(), 1);
        let marks = marks(&out);
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0].0, "cond_fn");
        assert_eq!(marks[0].1, groups[0].1[0]);
        // The body moved to top level under the private variant name, so only one definition of
        // the symbol is ever emitted no matter how many branches declare it.
        let hoisted = out
            .iter()
            .filter(|stmt| matches!(&stmt.kind, StmtKind::FunctionDecl { name, .. } if name == &groups[0].1[0]))
            .count();
        assert_eq!(hoisted, 1);
    }

    #[test]
    fn two_branches_declaring_one_name_share_a_group_and_get_distinct_symbols() {
        let program = vec![Stmt::new(
            StmtKind::If {
                condition: crate::parser::ast::Expr::new(
                    crate::parser::ast::ExprKind::BoolLiteral(true),
                    Span::dummy(),
                ),
                then_body: vec![function_decl("picked")],
                elseif_clauses: Vec::new(),
                else_body: Some(vec![function_decl("picked")]),
            },
            Span::dummy(),
        )];
        let out = bind_conditional_declarations(program, Target::detect_host());
        let groups = group_names(&out);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].1.len(), 2);
        assert_ne!(groups[0].1[0], groups[0].1[1]);
    }

    #[test]
    fn an_unconditional_declaration_of_the_same_name_keeps_the_conditional_one_untouched() {
        let program = vec![
            function_decl("both_ways"),
            guarded(vec![function_decl("both_ways")]),
        ];
        let out = bind_conditional_declarations(program, Target::detect_host());
        assert!(group_names(&out).is_empty());
        assert!(marks(&out).is_empty());
    }

    #[test]
    fn a_builtin_name_is_left_where_it_is() {
        let program = vec![guarded(vec![function_decl("strlen")])];
        let out = bind_conditional_declarations(program, Target::detect_host());
        assert!(group_names(&out).is_empty());
        assert!(marks(&out).is_empty());
    }

    #[test]
    fn an_unconditional_declaration_is_left_where_it_is() {
        let program = vec![function_decl("plain_fn")];
        let out = bind_conditional_declarations(program, Target::detect_host());
        assert!(group_names(&out).is_empty());
        assert!(marks(&out).is_empty());
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn a_namespaced_conditional_declaration_keeps_its_namespace_on_the_variant() {
        let program = vec![guarded(vec![function_decl("Scope\\cond_fn")])];
        let out = bind_conditional_declarations(program, Target::detect_host());
        let groups = group_names(&out);
        assert_eq!(groups[0].0, "Scope\\cond_fn");
        assert!(groups[0].1[0].starts_with("Scope\\__elephc_conditional_fn_"));
    }
}
