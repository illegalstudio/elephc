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
use crate::parser::ast::{BinOp, CastType, Expr, ExprKind, Program, Stmt, StmtKind};
use crate::synthetic_class::{
    e_array, e_binop, e_call, e_cast, e_index, e_int, e_str, e_var, function,
    php_semantics_declarations, s_array_assign, s_assign, s_expr, s_foreach, s_if, s_return,
    s_return_void, s_static, t_array, t_mixed,
};

/// Which `$GLOBALS` helpers the program calls.
#[derive(Default, Clone, Copy)]
struct Calls {
    snapshot: bool,
    set: bool,
    unset: bool,
}

fn calls(program: &Program) -> Calls {
    let mut found = Calls::default();
    walk_nodes(program, &mut |node| {
        if let Node::Expr(Expr { kind: ExprKind::FunctionCall { name, .. }, .. }) = node {
            match name.as_str().trim_start_matches('\\') {
                crate::globals_array::SNAPSHOT_FUNCTION => found.snapshot = true,
                crate::globals_dynamic::SET_FUNCTION => found.set = true,
                crate::globals_dynamic::UNSET_FUNCTION => found.unset = true,
                _ => {}
            }
        }
    });
    found
}

/// Whether the program reads or writes `$GLOBALS` through a computed key or as a whole array.
pub(crate) fn needed_by(program: &Program) -> bool {
    let found = calls(program);
    found.snapshot || found.set || found.unset
}

/// The global names the program can create, sorted for a stable declaration.
fn global_names(program: &Program) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut add = |name: &str| {
        if name != "this"
            && name != "GLOBALS"
            && !crate::superglobals::is_superglobal(name)
            && !name.starts_with(crate::globals_dynamic::TEMP_PREFIX)
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

/// The function holding the globals no top-level code names: a computed-key write creates them
/// and only a computed-key read or the whole array can see them.
const EXTRA_FUNCTION: &str = "__elephc_globals_extra";

/// The declarations of the `$GLOBALS` helpers `program` calls: the snapshot array, the
/// computed-key store and the computed-key unset.
pub(crate) fn declarations(program: &Program) -> Program {
    let names = global_names(program);
    let used = calls(program);
    // The names nothing in the program spells live in a side table, observable only through
    // the snapshot.
    let extras = used.snapshot && (used.set || used.unset);
    // php's `global $GLOBALS;` creates a global of that name holding null.
    let mut imports_globals = false;
    walk_nodes(program, &mut |node| {
        if let Node::Stmt(Stmt { kind: StmtKind::Global { vars }, .. }) = node {
            imports_globals |= vars.iter().any(|var| var == "GLOBALS");
        }
    });
    php_semantics_declarations(|| {
        let mut declared = Vec::new();
        if used.snapshot {
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
            if extras {
                body.push(s_foreach(
                    e_call(EXTRA_FUNCTION, Vec::new()),
                    Some("n"),
                    "v",
                    vec![s_array_assign("g", e_var("n"), e_var("v"))],
                ));
            }
            body.push(s_return(e_var("g")));
            declared.push(
                function(crate::globals_array::SNAPSHOT_FUNCTION)
                    .returns(t_array())
                    .body(body)
                    .build(),
            );
        }
        if extras {
            declared.push(
                function(EXTRA_FUNCTION)
                    .param_default("k", t_mixed(), e_str(""))
                    .param_default("v", t_mixed(), crate::synthetic_class::e_null())
                    .param_default("mode", t_mixed(), e_int(0))
                    .returns(t_array())
                    .body(vec![
                        s_static("e", e_array(Vec::new())),
                        s_if(
                            e_binop(e_var("mode"), BinOp::StrictEq, e_int(1)),
                            vec![s_array_assign("e", e_var("k"), e_var("v"))],
                            vec![(
                                e_binop(e_var("mode"), BinOp::StrictEq, e_int(2)),
                                vec![s_expr(e_call(
                                    "unset",
                                    vec![e_index(e_var("e"), e_var("k"))],
                                ))],
                            )],
                            None,
                        ),
                        s_return(e_var("e")),
                    ])
                    .build(),
            );
        }
        if used.set {
            let mut body = vec![s_assign("k", e_cast(CastType::String, e_var("k")))];
            for name in &names {
                body.push(s_if(
                    e_binop(e_var("k"), BinOp::StrictEq, e_str(name)),
                    vec![
                        s_assign(&crate::globals_array::alias_name(name), e_var("v")),
                        s_return(e_var("v")),
                    ],
                    vec![],
                    None,
                ));
            }
            if extras {
                body.push(s_expr(e_call(EXTRA_FUNCTION, vec![e_var("k"), e_var("v"), e_int(1)])));
            }
            body.push(s_return(e_var("v")));
            declared.push(
                function(crate::globals_dynamic::SET_FUNCTION)
                    .param("k", t_mixed())
                    .param("v", t_mixed())
                    .returns(t_mixed())
                    .body(body)
                    .build(),
            );
        }
        if used.unset {
            let mut body = vec![s_assign("k", e_cast(CastType::String, e_var("k")))];
            for name in &names {
                body.push(s_if(
                    e_binop(e_var("k"), BinOp::StrictEq, e_str(name)),
                    vec![
                        s_expr(e_call(
                            "unset",
                            vec![e_var(&crate::globals_array::alias_name(name))],
                        )),
                        s_return_void(),
                    ],
                    vec![],
                    None,
                ));
            }
            if extras {
                body.push(s_expr(e_call(EXTRA_FUNCTION, vec![e_var("k"), e_str(""), e_int(2)])));
            }
            declared.push(
                function(crate::globals_dynamic::UNSET_FUNCTION)
                    .param("k", t_mixed())
                    .body(body)
                    .build(),
            );
        }
        declared
    })
}
