//! Purpose:
//! Declares PHP 8.4's `array_find_key()` and `array_change_key_case()` as AST-built functions.
//! `array_first()` / `array_last()` are real builtins (`src/builtins/array/array_first.rs`), so
//! they must not be shadowed here: the builtin carries php's exact TypeError text.
//!
//! Called from:
//! - `crate::pipeline::compile()` in the compat-prelude phase, after autoload expansion, so a
//!   call only an autoloaded class makes is seen, and the codegen test harness.
//!
//! Key details:
//! - Both answer `null` for an empty array and otherwise the value at the first/last position
//!   in iteration order, never touching the array's internal pointer — php 8.5's contract.
//! - `array_find_key()` answers the key of the first element its predicate accepts, calling the
//!   predicate with the value and the key as php does, or `null`.
//! - They matter beyond direct callers: `symfony/polyfill-php85`'s bootstrap returns early on
//!   an 8.5 profile, so code generated against php 8.5 (a Symfony container dumping
//!   `\array_first(...)`) has no other definition; `symfony/polyfill-php84` does the same for
//!   `array_find_key()`, which Laravel's `Arr::first()` calls.
//! - Pay-for-use: injected only when the program names one of the functions, and never over a
//!   declaration of the same name the program makes itself.
//! - The interpreter has its own implementations
//!   (`elephc_magician::interpreter::builtins::array::array_first` / `array_find`), because a
//!   compiler prelude is never reachable from interpreted code.

use crate::parser::ast::{BinOp, Program, Stmt, StmtKind, TypeExpr};
use crate::synthetic_class::{
    e_array, e_binop, e_call, e_closure_call, e_index, e_int, e_null, e_ternary, e_var,
    function, php_semantics_declarations, s_array_assign, s_assign, s_foreach, s_if, s_return,
    t_array, t_mixed,
};

/// Prelude inventory group holding the declarations.
pub(crate) const GROUP: &str = "array_edge";

/// The PHP functions this prelude declares.
pub(crate) const FUNCTIONS: &[&str] = &[
    "array_find_key",
    "array_change_key_case",
];

/// Builds the two functions.
fn function_declarations() -> Vec<Stmt> {
    // function array_find_key(array $array, $callback): mixed {
    //     foreach ($array as $key => $value) {
    //         if ($callback($value, $key)) { return $key; }
    //     }
    //     return null;
    // }
    let find_key = function("array_find_key")
        .param("array", t_array())
        .param_untyped("callback")
        .returns(t_mixed())
        .body(vec![
            s_foreach(
                e_var("array"),
                Some("key"),
                "value",
                vec![s_if(
                    e_closure_call("callback", vec![e_var("value"), e_var("key")]),
                    vec![s_return(e_var("key"))],
                    Vec::new(),
                    None,
                )],
            ),
            s_return(e_null()),
        ])
        .build();

    // function array_change_key_case(array $array, int $case = 0 /* CASE_LOWER */): array {
    //     $result = [];
    //     foreach ($array as $key => $value) {
    //         if (is_string($key)) { $key = $case ? strtoupper($key) : strtolower($key); }
    //         $result[$key] = $value;
    //     }
    //     return $result;
    // }
    let change_key_case = function("array_change_key_case")
        .param("array", t_array())
        .param_default("case", TypeExpr::Int, e_int(0))
        .returns(t_array())
        .body(vec![
            s_assign("result", e_array(Vec::new())),
            s_foreach(
                e_var("array"),
                Some("key"),
                "value",
                vec![
                    s_if(
                        e_call("is_string", vec![e_var("key")]),
                        vec![s_assign(
                            "key",
                            e_ternary(
                                e_var("case"),
                                e_call("strtoupper", vec![e_var("key")]),
                                e_call("strtolower", vec![e_var("key")]),
                            ),
                        )],
                        Vec::new(),
                        None,
                    ),
                    s_array_assign("result", e_var("key"), e_var("value")),
                ],
            ),
            s_return(e_var("result")),
        ])
        .build();

    vec![find_key, change_key_case]
}

/// Returns the declarations to inject.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(function_declarations)
}

/// Returns whether a top-level, namespace-less, guarded or synthetic statement declares the
/// function `name`.
fn program_declares_function(program: &[Stmt], name: &str) -> bool {
    program.iter().any(|stmt| match &stmt.kind {
        StmtKind::FunctionDecl { name: declared, .. } => declared.eq_ignore_ascii_case(name),
        StmtKind::NamespaceBlock { name: None, body }
        | StmtKind::Synthetic(body)
        | StmtKind::IncludeOnceGuard { body, .. } => program_declares_function(body, name),
        _ => false,
    })
}

/// Prepends the declarations when the program names one of the functions and declares none of
/// them itself.
pub fn inject_if_used(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    let used = crate::prelude_prune::usage::collect(&program);
    let names_function = FUNCTIONS.iter().any(|name| {
        crate::opcache_prelude::detect::program_references(&program, name)
            || used.literals.contains(&crate::names::php_symbol_key(name))
    });
    if !names_function
        || FUNCTIONS
            .iter()
            .any(|name| program_declares_function(&program, name))
    {
        return program;
    }
    let mut combined = declarations();
    inventory.record_program(GROUP, &combined);
    combined.extend(program);
    combined
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The surface is exactly the declared functions.
    #[test]
    fn declares_array_first_array_last_and_array_find_key() {
        let functions: Vec<String> = declarations()
            .into_iter()
            .filter_map(|stmt| match stmt.kind {
                StmtKind::FunctionDecl { name, .. } => Some(name),
                _ => None,
            })
            .collect();
        assert_eq!(functions, FUNCTIONS);
    }
}
