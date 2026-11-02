//! Purpose:
//! Eval registry entry and implementation for `extract`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - `EXTR_*` mode values and the collision/prefix policy come from
//!   `crate::extract_policy::extract_target_name`, the SAME table the compiled backend's own
//!   eval-bridge `extract()` FFI (`crate::ffi::extract`) uses, so the two backends cannot
//!   silently diverge on a mode number or a collision rule.
//! - `EXTR_REFS` establishes a REAL PHP reference between the extracted variable and the source
//!   array's own element, reusing the same `eval_var_reference_bind` machinery `$b = &$arr['k'];`
//!   uses, but only when the source expression is a plain variable (`array &$array`'s own
//!   by-reference signature requires an addressable variable). `php -n` 8.5.6 raises no warning
//!   for any other source shape (an array literal, a call result, ...) -- it just binds by value,
//!   which is what happens here too when there is no variable name to alias.
//! - A non-array argument is elephc's own catchable `TypeError`; an out-of-range `$flags` is its
//!   own catchable `ValueError`, both worded exactly as `php -n` 8.5.6 words them for extract().

use super::super::super::*;
use crate::extract_policy::{extract_target_name, EXTR_IF_EXISTS, EXTR_OVERWRITE, EXTR_REFS};

// `source_arguments` routes the call to the direct hook below: it is the only implementation
// (`values: none`), and without it every `extract()` an eval fragment made took the
// evaluated-argument path and failed as an unsupported function -- Laravel's
// `Filesystem::getRequire()`, which renders every view and loads every config file.
eval_builtin! {
    contract: "extract",
    area: Array,
    source_arguments: true,
    direct: Extract,
    values: none,
}

/// Evaluates a direct, syntactic `extract(array, flags = EXTR_OVERWRITE, prefix = "")` call.
///
/// Only the syntactic call form is supported: `extract`'s first parameter is declared by
/// reference in php's own signature, and the collision/prefix policy plus `EXTR_REFS` both need
/// the calling SCOPE, which an already-evaluated-argument call site (`call_user_func("extract",
/// ...)`, a named-argument or spread call) does not thread through here.
pub(in crate::interpreter) fn eval_builtin_extract(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (array_expr, flags_expr, prefix_expr) = match args {
        [array] => (array, None, None),
        [array, flags] => (array, Some(flags), None),
        [array, flags, prefix] => (array, Some(flags), Some(prefix)),
        _ => return Err(EvalStatus::RuntimeFatal),
    };

    let array = eval_expr(array_expr, context, scope, values)?;
    let array_tag = values.type_tag(array)?;
    if !matches!(array_tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        let given = eval_given_type_spelling(array, values)?;
        return eval_throw_type_error(
            &format!("extract(): Argument #1 ($array) must be of type array, {given} given"),
            context,
            values,
        );
    }

    let flags = match flags_expr {
        Some(expr) => {
            let value = eval_expr(expr, context, scope, values)?;
            eval_int_value(value, values)?
        }
        None => EXTR_OVERWRITE,
    };
    let prefix = match prefix_expr {
        Some(expr) => {
            let value = eval_expr(expr, context, scope, values)?;
            String::from_utf8(values.string_bytes(value)?).map_err(|_| EvalStatus::RuntimeFatal)?
        }
        None => String::new(),
    };

    let mode = flags & !EXTR_REFS;
    if !(EXTR_OVERWRITE..=EXTR_IF_EXISTS).contains(&mode) {
        return eval_throw_builtin_value_error(
            "extract(): Argument #2 ($flags) must be a valid extract type",
            context,
            values,
        );
    }
    let by_ref = flags & EXTR_REFS != 0;
    // `array &$array` needs an addressable variable to alias; anything else (a literal, a call
    // result, ...) has no persistent storage to bind EXTR_REFS to, so it falls back to a value
    // bind below -- matching php, which raises no warning for that case either.
    let array_var_name = match array_expr {
        EvalExpr::LoadVar(name) => Some(name.clone()),
        _ => None,
    };

    let mut extracted = 0_i64;
    let len = values.array_len(array)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let Some((name, key_const)) = eval_extract_key_name_and_const(values, key)? else {
            values.release(key)?;
            continue;
        };
        let Some(target) = extract_target_name(scope, &name, &prefix, mode) else {
            values.release(key)?;
            continue;
        };
        if by_ref {
            if let Some(array_name) = &array_var_name {
                let source = EvalExpr::ArrayGet {
                    array: Box::new(EvalExpr::LoadVar(array_name.clone())),
                    index: Box::new(EvalExpr::Const(key_const)),
                };
                eval_var_reference_bind(&target, &source, context, scope, values)?;
                values.release(key)?;
                extracted += 1;
                continue;
            }
        }
        let value = values.array_get(array, key)?;
        values.release(key)?;
        if let Some(replaced) = scope.set(target, value, ScopeCellOwnership::Owned) {
            values.release(replaced)?;
        }
        extracted += 1;
    }

    values.int(extracted)
}

/// Converts one foreach-visible array key into both its PHP extraction name and a literal
/// `EvalConst` naming the same key, for `EXTR_REFS`'s synthetic `$array[$key]` reference source.
///
/// Mirrors `crate::ffi::extract::extract_key_name`'s int/string tag handling; kept separate
/// because that function returns only the name, not a reusable key constant.
fn eval_extract_key_name_and_const(
    values: &mut impl RuntimeValueOps,
    key: RuntimeCellHandle,
) -> Result<Option<(String, EvalConst)>, EvalStatus> {
    match values.type_tag(key)? {
        EVAL_TAG_STRING => {
            let name =
                String::from_utf8(values.string_bytes(key)?).map_err(|_| EvalStatus::RuntimeFatal)?;
            let constant = EvalConst::String(name.clone());
            Ok(Some((name, constant)))
        }
        EVAL_TAG_INT => {
            let n = values.raw_value_word(key)? as i64;
            Ok(Some((n.to_string(), EvalConst::Int(n))))
        }
        _ => Ok(None),
    }
}
