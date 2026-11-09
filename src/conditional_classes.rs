//! Purpose:
//! Gives a class-like declared inside a conditional statement the declaration it would have had
//! at file scope, when nothing else in the program can declare that name.
//!
//! Called from:
//! - `crate::pipeline::compile()` right after `crate::conditional_functions`, before type checking.
//!
//! Key details:
//! - php declares `if (...) { class C {} }` only when that statement executes. The AOT class table
//!   is built from top-level declarations, so such a class was simply missing: `new C` failed with
//!   "Undefined class", and `new $name` with "Class not found" at run time.
//! - A name declared EXACTLY ONCE in the whole program has one possible body, so declaring it
//!   unconditionally runs the same code php would run whenever php can run it. That covers the
//!   polyfill guard (`if (!class_exists(C::class)) { class C {} }`, whose condition then reads false
//!   and skips the now-empty branch) and a class declared under an arbitrary runtime condition.
//! - What differs from php is only the window before the branch runs, where `class_exists()` now
//!   answers true. A program that branches on that answer before declaring is the one shape this
//!   cannot model, and it is the same compromise the function pass avoids only because functions
//!   have a dispatcher thunk; a class has a layout every compiled access depends on.
//! - Two declarations of one name (mutually exclusive branches choosing a parent, say) are left
//!   where they are: picking one statically would run the wrong body half the time. Constant
//!   folding has already removed the branches a target decides, so the common
//!   `PHP_VERSION_ID`-style pair is gone before this pass sees it.

use crate::fast_hash::FastMap as HashMap;

use crate::names::php_symbol_key;
use crate::parser::ast::{Program, Stmt, StmtKind};

/// Hoists every conditionally declared class-like whose name the program declares only once.
pub fn hoist_unique_conditional_class_likes(program: Program) -> Program {
    let mut counts = HashMap::default();
    count_class_likes(&program, &mut counts);
    let mut hoister = Hoister { counts, hoisted: Vec::new() };
    let program = hoister.rewrite_stmts(program, false);
    if hoister.hoisted.is_empty() {
        return program;
    }
    let mut out = hoister.hoisted;
    out.extend(program);
    out
}

/// Returns the declared name of a class-like statement.
fn class_like_name(kind: &StmtKind) -> Option<&str> {
    match kind {
        StmtKind::ClassDecl { name, .. }
        | StmtKind::InterfaceDecl { name, .. }
        | StmtKind::TraitDecl { name, .. }
        | StmtKind::EnumDecl { name, .. } => Some(name),
        _ => None,
    }
}

/// Counts every class-like declaration by symbol key, wherever it sits.
fn count_class_likes(stmts: &[Stmt], counts: &mut HashMap<String, usize>) {
    for stmt in stmts {
        if let Some(name) = class_like_name(&stmt.kind) {
            *counts.entry(php_symbol_key(name)).or_default() += 1;
            continue;
        }
        for body in child_bodies(&stmt.kind) {
            count_class_likes(body, counts);
        }
    }
}

/// Borrows the statement lists nested directly in `kind` that can hold a class declaration.
///
/// Function and class bodies are deliberately absent: a class declared inside a function body is
/// bound per call, which hoisting would not model.
fn child_bodies(kind: &StmtKind) -> Vec<&Vec<Stmt>> {
    match kind {
        StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::Foreach { body, .. } => vec![body],
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            let mut bodies = vec![then_body];
            bodies.extend(elseif_clauses.iter().map(|(_, body)| body));
            bodies.extend(else_body.iter());
            bodies
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            let mut bodies = vec![then_body];
            bodies.extend(else_body.iter());
            bodies
        }
        StmtKind::Switch { cases, default, .. } => {
            let mut bodies = cases.iter().map(|(_, body)| body).collect::<Vec<_>>();
            bodies.extend(default.iter());
            bodies
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            let mut bodies = vec![try_body];
            bodies.extend(catches.iter().map(|catch| &catch.body));
            bodies.extend(finally_body.iter());
            bodies
        }
        _ => Vec::new(),
    }
}

/// Carries one rewrite over a whole program.
struct Hoister {
    /// Declarations per symbol key across the program.
    counts: HashMap<String, usize>,
    /// Declarations lifted to program top level, in source order.
    hoisted: Vec<Stmt>,
}

impl Hoister {
    /// Rewrites one statement list, dropping the declarations it lifts.
    fn rewrite_stmts(&mut self, stmts: Vec<Stmt>, conditional: bool) -> Vec<Stmt> {
        let mut out = Vec::with_capacity(stmts.len());
        for stmt in stmts {
            if conditional
                && class_like_name(&stmt.kind)
                    .is_some_and(|name| self.counts.get(&php_symbol_key(name)) == Some(&1))
            {
                self.hoisted.push(stmt);
                continue;
            }
            out.push(self.rewrite_stmt(stmt, conditional));
        }
        out
    }

    /// Rewrites the nested bodies of one statement.
    fn rewrite_stmt(&mut self, mut stmt: Stmt, conditional: bool) -> Stmt {
        let kind = std::mem::replace(&mut stmt.kind, StmtKind::Synthetic(Vec::new()));
        stmt.kind = self.rewrite_kind(kind, conditional);
        stmt
    }

    /// Rewrites the bodies of one statement kind; a transparent wrapper keeps `conditional`.
    fn rewrite_kind(&mut self, kind: StmtKind, conditional: bool) -> StmtKind {
        match kind {
            StmtKind::Synthetic(body) => StmtKind::Synthetic(self.rewrite_stmts(body, conditional)),
            StmtKind::NamespaceBlock { name, body } => StmtKind::NamespaceBlock {
                name,
                body: self.rewrite_stmts(body, conditional),
            },
            // A block folding made out of a target-decided guard inside an included file still
            // only runs with the include, so its class-likes are conditional: symfony/polyfill-
            // php86's `if (\PHP_VERSION_ID < 80600) { enum SortDirection {} }` stub, reached
            // through `require_once`. The file's own direct declarations stay where the include
            // machinery put them -- lifting those split a file like Symfony's
            // `ContainerConfigurator.php` from the functions it also declares, and the
            // interpreter could not run the remainder.
            StmtKind::IncludeOnceGuard { source_path, required, body } => StmtKind::IncludeOnceGuard {
                source_path,
                required,
                body: body
                    .into_iter()
                    .map(|mut stmt| match stmt.kind {
                        StmtKind::Synthetic(inner) => {
                            stmt.kind = StmtKind::Synthetic(self.rewrite_stmts(inner, true));
                            stmt
                        }
                        kind => {
                            stmt.kind = kind;
                            self.rewrite_stmt(stmt, conditional)
                        }
                    })
                    .collect(),
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::{Expr, ExprKind};
    use crate::span::Span;

    fn class_decl(name: &str) -> Stmt {
        Stmt::new(
            StmtKind::ClassDecl {
                generics: None,
                name: name.to_string(),
                doc_comment: None,
                extends: None,
                implements: Vec::new(),
                is_abstract: false,
                is_final: false,
                is_readonly_class: false,
                trait_uses: Vec::new(),
                properties: Vec::new(),
                methods: Vec::new(),
                constants: Vec::new(),
            },
            Span::dummy(),
        )
    }

    fn guarded(then_body: Vec<Stmt>, else_body: Option<Vec<Stmt>>) -> Stmt {
        Stmt::new(
            StmtKind::If {
                condition: Expr::new(ExprKind::BoolLiteral(true), Span::dummy()),
                then_body,
                elseif_clauses: Vec::new(),
                else_body,
            },
            Span::dummy(),
        )
    }

    fn top_level_names(program: &Program) -> Vec<&str> {
        program.iter().filter_map(|stmt| class_like_name(&stmt.kind)).collect()
    }

    /// A class declared once, under a condition, becomes a top-level declaration.
    #[test]
    fn a_unique_conditional_class_is_hoisted() {
        let out = hoist_unique_conditional_class_likes(vec![guarded(vec![class_decl("C")], None)]);
        assert_eq!(top_level_names(&out), vec!["C"]);
        let StmtKind::If { then_body, .. } = &out[1].kind else {
            panic!("the guard stays in place");
        };
        assert!(then_body.is_empty(), "the declaration left its branch");
    }

    /// Two declarations of one name keep their branches: neither body can be chosen statically.
    #[test]
    fn competing_conditional_declarations_stay_in_place() {
        let program = vec![guarded(vec![class_decl("C")], Some(vec![class_decl("c")]))];
        let out = hoist_unique_conditional_class_likes(program);
        assert!(top_level_names(&out).is_empty());
    }

    /// An unconditional declaration is already where the class table looks.
    #[test]
    fn an_unconditional_declaration_is_untouched() {
        let out = hoist_unique_conditional_class_likes(vec![class_decl("C")]);
        assert_eq!(top_level_names(&out), vec!["C"]);
        assert_eq!(out.len(), 1);
    }
}
