//! Purpose:
//! Eval registry entry and implementation for `levenshtein`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Byte-wise (not codepoint-wise) classic edit-distance dynamic program with three
//!   independent per-operation costs, matching php-src's own recurrence exactly -- including
//!   its complete lack of range validation on the cost arguments, which may be zero or negative
//!   (measured: `levenshtein("abc","b",-1)` is `2`, not a `ValueError`).
//! - This builtin has NO shared `BuiltinContract` predating this commit, no AOT registry
//!   binding, and no interpreter home file anywhere in this tree: the compiled backend serves
//!   only the two-argument call shape, and only when referenced, as a conditionally injected
//!   PHP-source prelude function (`__elephc_levenshtein_two_arg`,
//!   `src/backend_gap_prelude.rs`) that the eval bridge cannot reach at all. The contract is
//!   `BuiltinKind::PreludeProvided` in `catalog_surfaces.rs` for exactly that reason -- the
//!   `hash_init` shape, not a `catalog_data.rs` `Function` contract, which would wrongly demand
//!   an AOT `builtin!` registry binding the prelude route does not have.

use super::super::super::*;

eval_builtin! {
    contract: "levenshtein",
    area: String,
    direct: Levenshtein,
    values: Levenshtein,
}

/// Evaluates PHP `levenshtein(...)` from unevaluated call-site expressions.
pub(in crate::interpreter) fn eval_builtin_levenshtein(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() < 2 {
        return eval_throw_argument_count_error(
            &format!(
                "levenshtein() expects at least 2 arguments, {} given",
                args.len()
            ),
            context,
            values,
        );
    }
    if args.len() > 5 {
        return eval_throw_argument_count_error(
            &format!(
                "levenshtein() expects at most 5 arguments, {} given",
                args.len()
            ),
            context,
            values,
        );
    }
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_levenshtein_result(&evaluated, context, values)
}

/// Evaluates PHP `levenshtein(...)` from already-evaluated argument cells (the named/spread and
/// dynamic-call route).
pub(in crate::interpreter) fn eval_levenshtein_values(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_levenshtein_result(evaluated_args, context, values)
}

/// Computes PHP `levenshtein()` over already-evaluated arguments.
fn eval_levenshtein_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (string1, string2, insertion, replacement, deletion) = match evaluated_args {
        [s1, s2] => (*s1, *s2, None, None, None),
        [s1, s2, ins] => (*s1, *s2, Some(*ins), None, None),
        [s1, s2, ins, rep] => (*s1, *s2, Some(*ins), Some(*rep), None),
        [s1, s2, ins, rep, del] => (*s1, *s2, Some(*ins), Some(*rep), Some(*del)),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let bytes1 = eval_require_string_arg(string1, "levenshtein", 1, "string1", context, values)?;
    let bytes2 = eval_require_string_arg(string2, "levenshtein", 2, "string2", context, values)?;
    let insertion_cost = match insertion {
        Some(value) => {
            eval_require_int_arg(value, "levenshtein", 3, "insertion_cost", context, values)?
        }
        None => 1,
    };
    let replacement_cost = match replacement {
        Some(value) => {
            eval_require_int_arg(value, "levenshtein", 4, "replacement_cost", context, values)?
        }
        None => 1,
    };
    let deletion_cost = match deletion {
        Some(value) => {
            eval_require_int_arg(value, "levenshtein", 5, "deletion_cost", context, values)?
        }
        None => 1,
    };
    let distance = eval_levenshtein_distance(
        &bytes1,
        &bytes2,
        insertion_cost,
        replacement_cost,
        deletion_cost,
    );
    values.int(distance)
}

/// Computes the classic byte-wise edit distance dynamic program with independent per-operation
/// costs, matching php-src's own recurrence (including its lack of any range validation on the
/// cost arguments).
fn eval_levenshtein_distance(
    string1: &[u8],
    string2: &[u8],
    insertion_cost: i64,
    replacement_cost: i64,
    deletion_cost: i64,
) -> i64 {
    let width = string2.len();
    let mut previous: Vec<i64> = (0..=width as i64).map(|j| j * insertion_cost).collect();
    for &byte1 in string1 {
        let mut current = vec![0_i64; width + 1];
        current[0] = previous[0] + deletion_cost;
        for (col, &byte2) in string2.iter().enumerate() {
            let substitution = if byte1 == byte2 {
                previous[col]
            } else {
                previous[col] + replacement_cost
            };
            let deletion = previous[col + 1] + deletion_cost;
            let insertion = current[col] + insertion_cost;
            current[col + 1] = substitution.min(deletion).min(insertion);
        }
        previous = current;
    }
    previous[width]
}
