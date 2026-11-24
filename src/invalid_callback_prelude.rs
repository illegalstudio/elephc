//! Purpose:
//! Builds the one helper every unresolvable boxed callable reaches, so a callback that names no
//! invokable target raises php's catchable error instead of ending the program.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which declares it for every program.
//! - `crate::codegen::lower_inst::callables`, which calls it from the open dispatchers' miss
//!   paths (the scope-aware dispatcher, its inline ladder, the string and array resolvers).
//!
//! Key details:
//! - The wording follows `php -n` 8.5: a callback validated by a builtin raises a `TypeError`
//!   `<prefix>, <reason>` (prefix e.g. `call_user_func(): Argument #1 ($callback) must be a
//!   valid callback`), a callable invoked directly (`$f()`) raises an `Error` with a capitalised
//!   reason of its own. An empty prefix selects the direct call.
//! - Visibility is not decided here: an unreachable method has its own diagnostic at the miss
//!   site, which knows the calling scope. Only the reasons that need no scope are worded.
//! - php names an abstract method as its own reason, which no runtime builtin reports, so the
//!   program's abstract and interface methods are listed in the helper when there are any.
//! - The helper is BUILT, never parsed: elephc does not parse PHP source outside tests.

use crate::parser::ast::{BinOp, Expr, Program, Stmt, StmtKind, TypeExpr};
use crate::synthetic_class::{
    e_array, e_binop, e_bool, e_call, e_concat_all, e_index, e_int, e_new_fq, e_not, e_null,
    e_str, e_ternary, e_var, function, php_semantics_declarations, s_assign, s_if, s_return,
    s_array_push, s_foreach, s_throw, t_mixed,
};

/// Reserved name of the helper, shared with the lowering that calls it.
pub(crate) const INVALID_CALLBACK_HELPER_NAME: &str = "__elephc_invalid_callback";

/// Returns the helper declaration for injection, listing the program's abstract methods.
pub(crate) fn declarations(program: &Program) -> Program {
    let abstract_methods = abstract_method_keys(program);
    php_semantics_declarations(|| vec![helper(&abstract_methods)])
}

/// Lower-cased `class::method` keys of every abstract class method and interface method.
fn abstract_method_keys(program: &Program) -> Vec<String> {
    let mut keys = Vec::new();
    crate::parser::ast::visit::walk_stmts(program, &mut |stmt| {
        let (name, methods, interface) = match &stmt.kind {
            StmtKind::ClassDecl { name, methods, .. } => (name, methods, false),
            StmtKind::InterfaceDecl { name, methods, .. } => (name, methods, true),
            _ => return,
        };
        for method in methods {
            if method.is_abstract || (interface && !method.has_body) {
                keys.push(format!(
                    "{}::{}",
                    name.trim_start_matches('\\').to_ascii_lowercase(),
                    method.name.to_ascii_lowercase()
                ));
            }
        }
    });
    keys.sort();
    keys.dedup();
    keys
}

/// `$prefix === ""`: the callable was invoked directly rather than validated by a builtin.
fn is_direct() -> Expr {
    e_binop(e_var("prefix"), BinOp::StrictEq, e_str(""))
}

/// Throws the direct-call `Error` or the callback-validation `TypeError` with the given reasons.
fn fail(direct: Expr, validation: Expr) -> Vec<Stmt> {
    vec![
        s_if(is_direct(), vec![s_throw(e_new_fq("Error", vec![direct]))], vec![], None),
        s_throw(e_new_fq(
            "TypeError",
            vec![e_concat_all(vec![e_var("prefix"), e_str(", "), validation])],
        )),
    ]
}

/// `"<lead>" . <middle> . "<tail>"` for the wordings that quote a runtime name.
fn quoted(lead: &str, middle: Expr, tail: &str) -> Expr {
    e_concat_all(vec![e_str(lead), middle, e_str(tail)])
}

/// Builds `__elephc_invalid_callback(mixed $callback, string $prefix): mixed`.
fn helper(abstract_methods: &[String]) -> Stmt {
    let not_string_or_object = e_binop(
        e_not(e_call("is_string", vec![e_var("first")])),
        BinOp::And,
        e_not(e_call("is_object", vec![e_var("first")])),
    );
    let mut array_arm = vec![
        s_assign("callback", e_call("array_values", vec![e_var("callback")])),
        s_if(
            e_binop(e_call("count", vec![e_var("callback")]), BinOp::StrictNotEq, e_int(2)),
            fail(
                e_str("Array callback must have exactly two elements"),
                e_str("array callback must have exactly two members"),
            ),
            vec![],
            None,
        ),
        s_assign("first", e_index(e_var("callback"), e_int(0))),
        s_assign("method", e_index(e_var("callback"), e_int(1))),
        s_if(
            not_string_or_object,
            fail(
                e_str("First array member is not a valid class name or object"),
                e_str("first array member is not a valid class name or object"),
            ),
            vec![],
            None,
        ),
        s_if(
            e_not(e_call("is_string", vec![e_var("method")])),
            fail(
                e_str("Second array member is not a valid method"),
                e_str("second array member is not a valid method"),
            ),
            vec![],
            None,
        ),
        s_if(
            e_binop(
                e_call("is_string", vec![e_var("first")]),
                BinOp::And,
                e_binop(
                    e_not(e_call("class_exists", vec![e_var("first")])),
                    BinOp::And,
                    e_not(e_call("interface_exists", vec![e_var("first")])),
                ),
            ),
            fail(
                quoted("Class \"", e_var("first"), "\" not found"),
                quoted("class \"", e_var("first"), "\" not found"),
            ),
            vec![],
            None,
        ),
        s_assign(
            "class",
            e_ternary(
                e_call("is_object", vec![e_var("first")]),
                e_call("get_class", vec![e_var("first")]),
                e_var("first"),
            ),
        ),
        s_if(
            e_not(e_call("method_exists", vec![e_var("first"), e_var("method")])),
            fail(
                e_concat_all(vec![
                    e_str("Call to undefined method "),
                    e_var("class"),
                    e_str("::"),
                    e_var("method"),
                    e_str("()"),
                ]),
                e_concat_all(vec![
                    e_str("class "),
                    e_var("class"),
                    e_str(" does not have a method \""),
                    e_var("method"),
                    e_str("\""),
                ]),
            ),
            vec![],
            None,
        ),
    ];
    if !abstract_methods.is_empty() {
        let keys = e_array(abstract_methods.iter().map(|key| e_str(key)).collect());
        // The class and its ancestors, nearest first: the declaring class names the method.
        array_arm.push(s_assign("chain", e_array(vec![e_var("class")])));
        array_arm.push(s_foreach(
            e_call("class_parents", vec![e_var("class")]),
            None,
            "ancestor",
            vec![s_array_push("chain", e_var("ancestor"))],
        ));
        array_arm.push(s_foreach(
            e_var("chain"),
            None,
            "declaring",
            vec![
                s_if(
                    e_call(
                        "in_array",
                        vec![
                            e_concat_all(vec![
                                e_call("strtolower", vec![e_var("declaring")]),
                                e_str("::"),
                                e_call("strtolower", vec![e_var("method")]),
                            ]),
                            keys,
                        ],
                    ),
                    fail(
                        e_concat_all(vec![
                            e_str("Cannot call abstract method "),
                            e_var("declaring"),
                            e_str("::"),
                            e_var("method"),
                            e_str("()"),
                        ]),
                        e_concat_all(vec![
                            e_str("cannot call abstract method "),
                            e_var("declaring"),
                            e_str("::"),
                            e_var("method"),
                            e_str("()"),
                        ]),
                    ),
                    vec![],
                    None,
                ),
            ],
        ));
    }
    // Reachable only for a method that exists, is not abstract and is not reached from the
    // caller's scope as a static call: php words it as a non-static method called statically.
    array_arm.extend(fail(
        e_concat_all(vec![
            e_str("Non-static method "),
            e_var("class"),
            e_str("::"),
            e_var("method"),
            e_str("() cannot be called statically"),
        ]),
        e_concat_all(vec![
            e_str("non-static method "),
            e_var("class"),
            e_str("::"),
            e_var("method"),
            e_str("() cannot be called statically"),
        ]),
    ));

    let string_arm = fail(
        e_concat_all(vec![e_str("Call to undefined function "), e_var("callback"), e_str("()")]),
        quoted("function \"", e_var("callback"), "\" not found or invalid function name"),
    );
    let other_arm = fail(
        e_ternary(
            e_call("is_object", vec![e_var("callback")]),
            e_concat_all(vec![
                e_str("Object of type "),
                e_call("get_class", vec![e_var("callback")]),
                e_str(" is not callable"),
            ]),
            e_concat_all(vec![
                e_str("Value of type "),
                e_call("get_debug_type", vec![e_var("callback")]),
                e_str(" is not callable"),
            ]),
        ),
        e_str("no array or string given"),
    );
    let mut body = vec![s_if(
        e_binop(
            e_call("is_string", vec![e_var("callback")]),
            BinOp::And,
            e_binop(
                e_call("strpos", vec![e_var("callback"), e_str("::")]),
                BinOp::StrictNotEq,
                e_bool(false),
            ),
        ),
        vec![s_assign("callback", e_call("explode", vec![e_str("::"), e_var("callback"), e_int(2)]))],
        vec![],
        None,
    )];
    body.push(s_if(
        e_call("is_array", vec![e_var("callback")]),
        array_arm,
        vec![(e_call("is_string", vec![e_var("callback")]), string_arm)],
        Some(other_arm),
    ));
    body.push(s_return(e_null()));
    function(INVALID_CALLBACK_HELPER_NAME)
        .param_untyped("callback")
        .param("prefix", TypeExpr::Str)
        .returns(t_mixed())
        .body(body)
        .build()
}
