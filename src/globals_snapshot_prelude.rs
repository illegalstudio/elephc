//! Purpose:
//! Declares `__elephc_globals_array()`, the array a whole-array READ of `$GLOBALS` stands for
//! (`array_keys($GLOBALS)`, `$GLOBALS[$name]`; psysh's `GlobalVariableEnumerator`).
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, when the parser rewrote such a read
//!   (`crate::globals_array::snapshot_call`).
//!
//! Key details:
//! - A closed world knows every global the program can create: the variables its top-level code
//!   names, the names `global` imports and the `$GLOBALS['name']` it reads. The function lists
//!   each one that is set, in that order, through the `$GLOBALS['name']` alias, which routes to
//!   the global's own storage.
//! - A global holding null reads as absent (`isset`), and superglobals are not listed.
//! - The declarations are BUILT, never parsed.

use std::collections::BTreeSet;

use crate::parser::ast::visit::{walk_nodes, Node};
use crate::parser::ast::{Expr, ExprKind, Program, Stmt, StmtKind};
use crate::synthetic_class::{
    e_array, e_call, e_str, e_var, function, php_semantics_declarations, s_array_assign,
    s_assign, s_if, s_return, t_array,
};

/// Whether the program reads `$GLOBALS` as a whole array.
pub(crate) fn needed_by(program: &Program) -> bool {
    let mut found = false;
    walk_nodes(program, &mut |node| {
        if let Node::Expr(Expr { kind: ExprKind::FunctionCall { name, .. }, .. }) = node {
            found |= name.as_str().trim_start_matches('\\') == crate::globals_array::SNAPSHOT_FUNCTION;
        }
    });
    found
}

/// The global names the program can create, sorted for a stable declaration.
fn global_names(program: &Program) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut add = |name: &str| {
        if name != "this"
            && name != "GLOBALS"
            && !crate::superglobals::is_superglobal(name)
            && crate::globals_array::is_php_variable_name(name)
        {
            names.insert(name.to_string());
        }
    };
    // Top-level code, not the bodies the program declares.
    let top_level: Vec<Stmt> = program
        .iter()
        .flat_map(top_level_code)
        .collect();
    walk_nodes(&top_level, &mut |node| match node {
        Node::Expr(Expr { kind: ExprKind::Variable(name), .. }) => add(name),
        // The statements that bind a name spell it as a string, not a variable expression.
        Node::Stmt(stmt) => match &stmt.kind {
            StmtKind::Assign { name, .. }
            | StmtKind::TypedAssign { name, .. }
            | StmtKind::RefAssign { target: name, .. }
            | StmtKind::ArrayAssign { array: name, .. }
            | StmtKind::ArrayPush { array: name, .. } => add(name),
            StmtKind::Foreach { key_var, value_var, .. } => {
                add(value_var);
                if let Some(key) = key_var {
                    add(key);
                }
            }
            _ => {}
        },
        _ => {}
    });
    // `global $x` anywhere, and `$GLOBALS['x']` anywhere.
    walk_nodes(program, &mut |node| match node {
        Node::Stmt(Stmt { kind: StmtKind::Global { vars }, .. }) => {
            for var in vars {
                add(var);
            }
        }
        Node::Expr(Expr { kind: ExprKind::Variable(name), .. }) => {
            if let Some(target) = crate::globals_array::alias_target(name) {
                add(target);
            }
        }
        _ => {}
    });
    names
}

/// The statements of `stmt` that are top-level code: a namespace's or a spliced file's body, but
/// no function, class-like or other declaration body.
fn top_level_code(stmt: &Stmt) -> Vec<Stmt> {
    match &stmt.kind {
        StmtKind::NamespaceBlock { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::IncludeOnceGuard { body, .. } => body.iter().flat_map(top_level_code).collect(),
        StmtKind::FunctionDecl { .. }
        | StmtKind::ClassDecl { .. }
        | StmtKind::InterfaceDecl { .. }
        | StmtKind::TraitDecl { .. }
        | StmtKind::EnumDecl { .. } => Vec::new(),
        _ => vec![stmt.clone()],
    }
}

/// `function __elephc_globals_array(): array`, listing `program`'s set globals.
pub(crate) fn declarations(program: &Program) -> Program {
    let names = global_names(program);
    // php's `global $GLOBALS;` creates a global of that name holding null.
    let mut imports_globals = false;
    walk_nodes(program, &mut |node| {
        if let Node::Stmt(Stmt { kind: StmtKind::Global { vars }, .. }) = node {
            imports_globals |= vars.iter().any(|var| var == "GLOBALS");
        }
    });
    php_semantics_declarations(|| {
        let mut body = vec![s_assign("g", e_array(Vec::new()))];
        if imports_globals {
            body.push(s_array_assign("g", e_str("GLOBALS"), crate::synthetic_class::e_null()));
        }
        for name in &names {
            let global = e_var(&crate::globals_array::alias_name(name));
            body.push(s_if(
                e_call("isset", vec![global.clone()]),
                vec![s_array_assign("g", e_str(name), global)],
                vec![],
                None,
            ));
        }
        body.push(s_return(e_var("g")));
        vec![function(crate::globals_array::SNAPSHOT_FUNCTION)
            .returns(t_array())
            .body(body)
            .build()]
    })
}
