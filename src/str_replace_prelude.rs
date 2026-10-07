//! Purpose:
//! Injects the elephc-PHP helpers behind the array forms of `str_replace()` and
//! `str_ireplace()`: an array `$search` (with a string or array `$replace`), an array
//! `$subject`, and the by-reference `$count` total.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness via `inject_if_used`, AFTER
//!   `autoload::run`, so a call that only appears in an autoloaded class file is detected,
//!   and after `func_args::desugar`, so the helpers keep exactly the parameters the lowering
//!   passes (a program with `eval()` gives every desugared function a hidden collector).
//! - `crate::builtins::string::str_replace_support`, whose EIR lowering calls the helpers by
//!   name when a call site needs more than the three-string runtime fast path.
//! - `crate::optimize::reachability`, which roots [`HELPERS`] only when the checker reports
//!   (`CheckResult::string_replace_helpers`) that some call cannot use the runtime fast path,
//!   so a program whose calls are all three plain strings carries none of them.
//!
//! Key details:
//! - Only the composition lives here. Each individual replacement still runs through the
//!   scalar `str_replace()`/`str_ireplace()` runtime helper, which the helper bodies reach
//!   with three string operands, so they never recurse into themselves.
//! - php-src semantics reproduced: search entries apply in iteration order to the result of
//!   the previous entry; a `$replace` array is consumed in its own iteration order and runs
//!   out to `""`; an empty search entry is skipped; an empty subject stops the walk; every
//!   `$subject` element is string-converted (a nested array becomes `"Array"` with PHP's
//!   warning) and the result keeps the subject's keys; a string `$search` with an array
//!   `$replace` throws php-src's `TypeError`, from [`CHECK_HELPER`] before anything is boxed.
//! - The replacement count is computed per step with `substr_count()` on the same operands
//!   the replacement uses (ASCII-lowered for the case-insensitive form), which is exactly the
//!   number of non-overlapping matches the runtime replaces.
//! - The total travels through [`COUNT_HELPER`]'s `static` slot instead of a by-reference
//!   parameter: a by-reference parameter gives a helper an `Unknown` return-alias summary,
//!   which pins every argument temporary at its call site. Each top-level helper stores its
//!   total once, after all nested `__toString()` calls are done, and the lowering reads it
//!   right after the call.
//! - Every helper is a single body with no nested user-function call carrying heap values,
//!   and every return value is a fresh local, so no return can alias a parameter.

use crate::errors::CompileError;
use crate::opcache_prelude::detect::{self, Symbol};
use crate::parser::ast::{BinOp, CastType, Expr, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_array, e_binop, e_bool, e_call, e_cast, e_concat, e_index, e_int, e_new, e_not, e_str,
    e_ternary, e_var, function, internal_declarations, s_array_assign, s_array_push, s_assign,
    s_break, s_expr, s_foreach, s_if, s_return, s_static, s_throw, t_mixed,
};

/// The helper the lowering calls when the subject is statically not an array: `: string`.
pub(crate) const STRING_HELPER: &str = "__elephc_str_replace_string";

/// The helper the lowering calls when the subject may be an array: `: mixed`.
pub(crate) const MIXED_HELPER: &str = "__elephc_str_replace_mixed";

/// The helper that stores (`$op === 0`) and reads back (`$op === 1`) the last total.
pub(crate) const COUNT_HELPER: &str = "__elephc_str_replace_count";

/// The helper that throws php-src's `TypeError` for a string `$search` with an array
/// `$replace`. It takes the two `is_array()` answers, never the operands themselves.
pub(crate) const CHECK_HELPER: &str = "__elephc_str_replace_check";

/// Every helper name, in declaration order. Declaration reachability roots all of them when
/// the checker reports that some call needs them.
pub(crate) const HELPERS: [&str; 4] = [COUNT_HELPER, CHECK_HELPER, STRING_HELPER, MIXED_HELPER];

/// The builtins whose lowering may call these helpers.
pub(crate) const CALLERS: [&str; 2] = ["str_replace", "str_ireplace"];

/// `__elephc_str_replace_count(int $op, int $value): int` — the last call's total.
fn count_helper_decl() -> Stmt {
    function(COUNT_HELPER)
        .param("op", TypeExpr::Int)
        .param("value", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            s_static("total", e_int(0)),
            s_if(
                e_binop(e_var("op"), BinOp::StrictEq, e_int(0)),
                vec![s_assign("total", e_var("value"))],
                vec![],
                None,
            ),
            s_return(e_var("total")),
        ])
        .build()
}

/// php-src's `TypeError` for a string `$search` paired with an array `$replace`.
fn replace_type_error() -> Expr {
    e_concat(
        e_ternary(e_var("ci"), e_str("str_ireplace"), e_str("str_replace")),
        e_str("(): Argument #2 ($replace) must be of type string when argument #1 ($search) is a string"),
    )
}

/// `__elephc_str_replace_check(bool $searchIsArray, bool $replaceIsArray, bool $ci): bool`.
///
/// Runs BEFORE the operands are boxed for the main helpers: its arguments are plain booleans,
/// so a throw from here strands no heap value, where a throw from inside a main helper would
/// leave the caller's operand boxes unreleased.
fn check_helper_decl() -> Stmt {
    function(CHECK_HELPER)
        .param("searchIsArray", TypeExpr::Bool)
        .param("replaceIsArray", TypeExpr::Bool)
        .param("ci", TypeExpr::Bool)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_not(e_var("searchIsArray")), BinOp::And, e_var("replaceIsArray")),
                vec![s_throw(e_new("TypeError", vec![replace_type_error()]))],
                vec![],
                None,
            ),
            s_return(e_bool(true)),
        ])
        .build()
}

/// Flattens `$search` into the string list `$needles` and `$replace` into either the value
/// list `$withs` (with its count) or the single string `$replacement`, and starts the
/// `$total` counter. The operand shapes were already validated by [`CHECK_HELPER`].
fn normalize_operands() -> Vec<Stmt> {
    vec![
        s_assign("needles", e_array(vec![])),
        s_if(
            e_call("is_array", vec![e_var("search")]),
            vec![s_foreach(
                e_var("search"),
                None,
                "searchEntry",
                vec![s_array_push("needles", e_cast(CastType::String, e_var("searchEntry")))],
            )],
            vec![],
            Some(vec![s_array_push("needles", e_cast(CastType::String, e_var("search")))]),
        ),
        s_assign("withs", e_array(vec![])),
        s_assign("replacement", e_str("")),
        s_assign("replaceIsArray", e_call("is_array", vec![e_var("replace")])),
        s_if(
            e_var("replaceIsArray"),
            vec![s_foreach(
                e_var("replace"),
                None,
                "replaceEntry",
                // Kept as raw values and string-converted where each is used: pushing the cast
                // string and reading it back by index inside the walk leaks one copy per entry.
                vec![s_array_push("withs", e_var("replaceEntry"))],
            )],
            vec![],
            Some(vec![s_assign("replacement", e_cast(CastType::String, e_var("replace")))]),
        ),
        s_assign("withCount", e_call("count", vec![e_var("withs")])),
        s_assign("total", e_int(0)),
    ]
}

/// Applies every search entry, in order, to `$result`, adding each step's match count to
/// `$total`. The needle's position `$i` also indexes the replacement list, so a skipped
/// empty needle still consumes its replacement, as in php-src.
///
/// Every operand of the nested `str_replace()`/`str_ireplace()` call must be typed `string`,
/// or the call would leave the runtime fast path and come back into these helpers (and keep
/// them alive in every program). Two checker widenings stand in the way: an indexed element
/// read inside a loop body is `mixed`, so the needles are walked with `foreach`; and the
/// foreach value of a walk nested in the subject loop is `mixed` too, so the call takes the
/// needle through an explicitly cast `$needleText`.
fn replace_loop(suffix: &str) -> Stmt {
    let local = |name: &str| format!("{name}{suffix}");
    let lowered = |name: &str| e_call("strtolower", vec![e_var(name)]);
    let needle = local("needle");
    let replace_call = |name: &str| {
        e_call(name, vec![e_var(&local("needleText")), e_var(&local("with")), e_var("result")])
    };
    s_foreach(
        e_var("needles"),
        Some(&local("i")),
        &needle,
        vec![
            s_if(
                e_binop(e_var("result"), BinOp::StrictEq, e_str("")),
                vec![s_break(1)],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var(&needle), BinOp::StrictNotEq, e_str("")),
                vec![
                    s_assign(&local("needleText"), e_cast(CastType::String, e_var(&needle))),
                    s_assign(&local("with"), e_var("replacement")),
                    s_if(
                        e_binop(
                            e_var("replaceIsArray"),
                            BinOp::And,
                            e_binop(e_var(&local("i")), BinOp::Lt, e_var("withCount")),
                        ),
                        vec![s_assign(
                            &local("with"),
                            e_cast(CastType::String, e_index(e_var("withs"), e_var(&local("i")))),
                        )],
                        vec![],
                        None,
                    ),
                    s_assign(
                        &local("hits"),
                        e_ternary(
                            e_var("ci"),
                            e_call("substr_count", vec![lowered("result"), lowered(&needle)]),
                            e_call("substr_count", vec![e_var("result"), e_var(&needle)]),
                        ),
                    ),
                    s_if(
                        e_binop(e_var(&local("hits")), BinOp::Gt, e_int(0)),
                        vec![
                            s_assign(
                                "total",
                                e_binop(e_var("total"), BinOp::Add, e_var(&local("hits"))),
                            ),
                            s_assign(
                                "result",
                                e_ternary(
                                    e_var("ci"),
                                    replace_call("str_ireplace"),
                                    replace_call("str_replace"),
                                ),
                            ),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
        ],
    )
}

/// `$result = (string) <value>;` followed by the replacement walk over it.
///
/// Each walk in one helper gets its own `suffix` for its loop locals, so the mixed helper's
/// two walks (one of them nested in the subject loop) never merge their loop variables'
/// types into a wider one.
fn replace_one(value: Expr, suffix: &str) -> Vec<Stmt> {
    vec![s_assign("result", e_cast(CastType::String, value)), replace_loop(suffix)]
}

/// Stores the call's total for the lowering to read back.
fn publish_total() -> Stmt {
    s_expr(e_call(COUNT_HELPER, vec![e_int(0), e_var("total")]))
}

/// Starts a helper with the shared `(mixed $search, mixed $replace, mixed $subject, bool $ci)`
/// parameter list.
fn helper_signature(name: &str) -> crate::synthetic_class::FunctionBuilder {
    function(name)
        .param("search", t_mixed())
        .param("replace", t_mixed())
        .param("subject", t_mixed())
        .param("ci", TypeExpr::Bool)
}

/// `__elephc_str_replace_string(...): string` — a subject that is not an array.
fn string_helper_decl() -> Stmt {
    let mut body = normalize_operands();
    body.extend(replace_one(e_var("subject"), ""));
    body.push(publish_total());
    body.push(s_return(e_var("result")));
    helper_signature(STRING_HELPER).returns(TypeExpr::Str).body(body).build()
}

/// `__elephc_str_replace_mixed(...): mixed` — a subject that may be an array, whose result
/// is an array with the subject's keys, or a string for a runtime string subject.
fn mixed_helper_decl() -> Stmt {
    let mut body = normalize_operands();
    let mut scalar = replace_one(e_var("subject"), "");
    scalar.push(publish_total());
    scalar.push(s_return(e_var("result")));
    body.push(s_if(e_not(e_call("is_array", vec![e_var("subject")])), scalar, vec![], None));
    body.push(s_assign("out", e_array(vec![])));
    let mut element = replace_one(e_var("value"), "Element");
    element.push(s_array_assign("out", e_var("key"), e_var("result")));
    body.push(s_foreach(e_var("subject"), Some("key"), "value", element));
    body.push(publish_total());
    body.push(s_return(e_var("out")));
    helper_signature(MIXED_HELPER).returns(t_mixed()).body(body).build()
}

/// Builds the four helpers.
pub(crate) fn str_replace_declarations() -> Program {
    internal_declarations(|| {
        vec![count_helper_decl(), check_helper_decl(), string_helper_decl(), mixed_helper_decl()]
    })
}

/// Returns whether the program references `str_replace()` or `str_ireplace()` anywhere: a
/// call, or a string literal naming it (`call_user_func('str_replace', ...)`, a callable
/// string in a variable), which reaches the same lowering.
fn program_calls_string_replace(program: &[Stmt]) -> bool {
    CALLERS
        .iter()
        .any(|name| detect::first_reference(program, Symbol::function(name)).is_some())
}

/// Prepends the helpers when the program calls `str_replace()`/`str_ireplace()`; otherwise
/// returns the program unchanged. The declarations are function declarations only, so
/// prepending does not change top-level execution order, and declaration reachability drops
/// them again when no surviving call needs them.
///
/// Injection runs AFTER the pipeline's name-resolution pass (like `object_cast_prelude`), so
/// the declarations are resolved here the way `autoload::run` resolves the files it splices.
/// A program that declares one of the helper names itself is rejected: the lowering calls
/// those names, so a user definition would silently become the builtin's semantics.
///
/// Runs under `crate::compiler_stack::with_compiler_stack`, because the usage scan walks the
/// whole program.
pub fn inject_if_used(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Result<Program, CompileError> {
    crate::compiler_stack::with_compiler_stack(|| {
        if !program_calls_string_replace(&program) {
            return Ok(program);
        }
        for helper in HELPERS {
            if let Some(span) = detect::first_declaration(&program, helper) {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Cannot declare {}(): the name is reserved for the compiler's \
                         str_replace() helper. Rename the function.",
                        helper
                    ),
                ));
            }
        }
        let mut combined = crate::name_resolver::resolve(str_replace_declarations())?;
        inventory.record_program("str_replace", &combined);
        combined.extend(program);
        Ok(combined)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::StmtKind;

    /// The prelude declares exactly the four helpers the lowering and reachability name.
    #[test]
    fn declares_every_helper_the_lowering_calls() {
        let declared: Vec<String> = str_replace_declarations()
            .iter()
            .filter_map(|stmt| match &stmt.kind {
                StmtKind::FunctionDecl { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(declared, HELPERS.to_vec());
    }

    /// Unused helpers are not injected into a program that never calls either builtin.
    #[test]
    fn programs_without_a_call_are_left_alone() {
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let program = inject_if_used(Vec::new(), &mut inventory).expect("empty program");
        assert!(program.is_empty());
        assert!(inventory.groups.is_empty());
    }
}
