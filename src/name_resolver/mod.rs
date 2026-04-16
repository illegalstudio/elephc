mod expressions;
mod names;
mod declarations;
mod statements;
mod symbols;

use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::names::{Name, NameKind};
use crate::parser::ast::{Expr, ExprKind, Program};

#[derive(Default, Clone)]
struct Imports {
    classes: HashMap<String, String>,
    functions: HashMap<String, String>,
    constants: HashMap<String, String>,
}

#[derive(Default)]
struct Symbols {
    functions: HashSet<String>,
    classes: HashSet<String>,
    interfaces: HashSet<String>,
    traits: HashSet<String>,
    constants: HashSet<String>,
    extern_functions: HashSet<String>,
    extern_classes: HashSet<String>,
}

pub fn resolve(program: Program) -> Result<Program, CompileError> {
    let mut symbols = Symbols::default();
    symbols::collect_symbols(&program, None, &mut symbols);
    statements::resolve_stmt_list(&program, None, &Imports::default(), &symbols)
}

fn rewrite_callback_literal_args(
    function_name: &str,
    args: &[Expr],
    current_namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> Vec<Expr> {
    let callback_positions: &[usize] = match function_name {
        "function_exists" | "call_user_func" | "call_user_func_array" => &[0],
        "array_map" | "array_filter" | "array_reduce" | "array_walk" => &[0],
        "usort" | "uksort" | "uasort" => &[1],
        _ => &[],
    };

    args.iter()
        .enumerate()
        .map(|(idx, arg)| {
            if callback_positions.contains(&idx) {
                if let ExprKind::StringLiteral(raw_name) = &arg.kind {
                    let resolved = names::resolve_function_name(
                        &parse_callback_name(raw_name),
                        current_namespace,
                        imports,
                        symbols,
                    );
                    return Expr::new(ExprKind::StringLiteral(resolved), arg.span);
                }
            }
            arg.clone()
        })
        .collect()
}

fn parse_callback_name(raw_name: &str) -> Name {
    if let Some(stripped) = raw_name.strip_prefix('\\') {
        return Name::from_parts(
            NameKind::FullyQualified,
            stripped.split('\\').map(str::to_string).collect(),
        );
    }
    if raw_name.contains('\\') {
        return Name::from_parts(
            NameKind::Qualified,
            raw_name.split('\\').map(str::to_string).collect(),
        );
    }
    Name::unqualified(raw_name)
}

fn resolved_name(name: String) -> Name {
    Name::from_parts(
        NameKind::FullyQualified,
        name.split('\\').map(str::to_string).collect(),
    )
}

fn namespace_name(name: &Option<Name>) -> String {
    name.as_ref().map(Name::as_canonical).unwrap_or_default()
}

pub(crate) fn is_builtin_function(name: &str) -> bool {
    crate::types::checker::builtins::is_supported_builtin_function(name)
}
