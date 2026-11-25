//! Purpose:
//! Decides which declaration PHP keeps for a class, interface, trait or enum name declared more
//! than once, and reports every other declaration of it as a redeclaration.
//!
//! Called from:
//! - `crate::types::checker::driver::check_types_impl()` before trait flattening, whose trait
//!   table, the class and interface maps, the enum builds and the trait requirement checks then
//!   skip the rejected statements.
//!
//! Key details:
//! - PHP binds a declaration while compiling the file (early binding) when it is a trait without
//!   `use`, an interface without parents, or a class that uses no trait, implements no interface,
//!   and extends nothing or a parent bound before it; it never early-binds an enum. Every other
//!   declaration is bound when PHP runs it, in source order.
//! - The first declaration bound keeps the name: the first early-bound one in source order, else
//!   the first one. Every other declaration of the name fails when PHP declares it (`Cannot
//!   redeclare ...`), so its error is at its own statement and ordered there, as a declaration
//!   PHP links at run time.
//! - A parent is bound when an earlier declaration of it is early-bound, or, with no earlier
//!   declaration, when it is a builtin class-like (`elephc_builtin_contract::lookup_class`); an
//!   unknown parent leaves the class to run time.

use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{Program, Stmt, StmtKind};
use crate::span::Span;

use super::super::schema::LinkOrder;

/// The declarations PHP rejects for reusing a name, with their errors.
pub(super) struct Redeclarations {
    /// The rejected declaration statements, by span.
    pub(super) rejected: HashSet<Span>,
    /// One error per rejected statement, at the statement and ordered there.
    pub(super) errors: Vec<(LinkOrder, CompileError)>,
}

/// One class-like declaration statement in source order.
struct Declaration<'a> {
    stmt: &'a Stmt,
    name: &'a str,
    key: String,
    early_bound: bool,
}

/// Finds the declarations of `program` that reuse a name PHP binds to another declaration (see
/// the module notes), numbered like `declaration_statement_positions`.
pub(super) fn redeclarations(program: &Program) -> Redeclarations {
    let mut declarations: Vec<Declaration<'_>> = Vec::new();
    collect_declarations(program, &mut declarations);
    // The keys declared so far, and those with an early-bound declaration so far, in source
    // order: a parent is bound when an earlier declaration of it is, or, with none earlier, when
    // it is a builtin class-like (a later declaration does not hide the builtin).
    let mut seen: HashSet<String> = HashSet::new();
    let mut bound: HashSet<String> = HashSet::new();
    for index in 0..declarations.len() {
        let early_bound = match &declarations[index].stmt.kind {
            StmtKind::TraitDecl { trait_uses, .. } => trait_uses.is_empty(),
            StmtKind::InterfaceDecl { extends, .. } => extends.is_empty(),
            StmtKind::ClassDecl { extends, implements, trait_uses, .. } => {
                trait_uses.is_empty()
                    && implements.is_empty()
                    && extends.as_ref().is_none_or(|parent| {
                        let key = php_symbol_key(parent.as_str());
                        if seen.contains(&key) {
                            bound.contains(&key)
                        } else {
                            elephc_builtin_contract::lookup_class(parent.as_str()).is_some()
                        }
                    })
            }
            _ => false,
        };
        declarations[index].early_bound = early_bound;
        seen.insert(declarations[index].key.clone());
        if early_bound {
            bound.insert(declarations[index].key.clone());
        }
    }
    let mut winners: HashMap<&str, usize> = HashMap::new();
    for (index, declaration) in declarations.iter().enumerate() {
        let winner = winners.entry(declaration.key.as_str()).or_insert(index);
        if declaration.early_bound && !declarations[*winner].early_bound {
            *winner = index;
        }
    }
    let mut rejected = HashSet::new();
    let mut errors = Vec::new();
    for (position, declaration) in declarations.iter().enumerate() {
        if winners.get(declaration.key.as_str()) == Some(&position) {
            continue;
        }
        let kind = match declaration.stmt.kind {
            StmtKind::InterfaceDecl { .. } => "interface",
            StmtKind::TraitDecl { .. } => "trait",
            StmtKind::EnumDecl { .. } => "class or enum",
            _ => "class",
        };
        rejected.insert(declaration.stmt.span);
        errors.push((
            (true, position),
            CompileError::new(
                declaration.stmt.span,
                &format!("Duplicate {} declaration: {}", kind, declaration.name),
            ),
        ));
    }
    Redeclarations { rejected, errors }
}

/// Collects the class, interface, trait and enum declaration statements of `stmts` in source
/// order, through namespace blocks, as `declaration_statement_positions` numbers them.
fn collect_declarations<'a>(stmts: &'a [Stmt], declarations: &mut Vec<Declaration<'a>>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::ClassDecl { name, .. }
            | StmtKind::InterfaceDecl { name, .. }
            | StmtKind::EnumDecl { name, .. }
            | StmtKind::TraitDecl { name, .. } => declarations.push(Declaration {
                stmt,
                name,
                key: php_symbol_key(name),
                early_bound: false,
            }),
            StmtKind::NamespaceBlock { body, .. } => collect_declarations(body, declarations),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::redeclarations;

    /// The messages of the declarations `source` rejects, in source order.
    fn rejected_messages(source: &str) -> Vec<String> {
        let tokens = crate::lexer::tokenize(source).expect("test source must tokenize");
        let program = crate::parser::parse(&tokens).expect("test source must parse");
        redeclarations(&program).errors.into_iter().map(|(_, error)| error.message).collect()
    }

    /// A child of a builtin early-binds before a later declaration of the builtin's name, so the
    /// later declaration does not hide the builtin: the class keeps `E` (elephc rejects the
    /// redeclared builtin itself before checking).
    #[test]
    fn later_declaration_does_not_hide_a_builtin_parent() {
        assert_eq!(
            rejected_messages("<?php enum E { case A; } class E extends Exception {} class Exception {}"),
            ["Duplicate class or enum declaration: E"],
        );
        assert_eq!(
            rejected_messages("<?php interface E extends Iterator {} class E extends Exception {} class Exception {}"),
            ["Duplicate interface declaration: E"],
        );
    }
}
