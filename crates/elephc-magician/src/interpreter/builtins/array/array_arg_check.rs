//! Purpose:
//! Shared PHP `TypeError` validation for the array-taking builtin argument family.
//!
//! Called from:
//! - `crate::interpreter::builtins::array` leaf builtins and the area dispatchers.
//!
//! Key details:
//! - Mirrors the compiled backend's `ARRAY_OR_FALSE_ARG_SITES` table, including PHP's
//!   variadic naming rules, so eval and codegen word the same throw identically.
//! - Type names are PHP's DEBUG spellings, not `gettype()`'s legacy ones.

use super::super::super::*;

/// What one argument slot DECLARES, which decides both its wording and what it accepts.
///
/// PHP does not word this family with one rule, so the table has to carry the kind next
/// to the name. Every spelling below was read out of a `php -n` 8.5.6 message.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::interpreter) enum EvalArrayArgKind {
    /// PHP's plain `array` declaration: only an array passes.
    Array,
    /// PHP's `?array` declaration, as `implode()` carries.
    ///
    /// The `?` stays in the message, and `null` PASSES this check — PHP rejects it one
    /// stage later with an entirely different sentence, which the caller words itself.
    NullableArray,
    /// PHP's `Traversable|array`, as the `iterator_*` helpers carry.
    ///
    /// An object passes only when it implements `Traversable`; a plain one is named by
    /// its CLASS against the union type.
    TraversableOrArray,
    /// PHP's `array|object`, as `current()` and `key()` carry.
    ///
    /// EVERY object passes — `current(new stdClass())` answers `false` rather than
    /// throwing, measured — yet the failure message still says plain `array`, because
    /// PHP words it from the arg_info alone. That mismatch is PHP's, not ours.
    ArrayOrObject,
}

impl EvalArrayArgKind {
    /// Returns PHP's spelling of the declared type for the `TypeError` message.
    fn expected(self) -> &'static str {
        match self {
            Self::Array | Self::ArrayOrObject => "array",
            Self::NullableArray => "?array",
            Self::TraversableOrArray => "Traversable|array",
        }
    }
}

/// The array-taking builtin arguments eval validates, with PHP's own parameter naming.
///
/// Each entry is `(builtin, zero-based argument index, PHP's parameter name, kind)`. A
/// `None` name is PHP's VARIADIC spelling, which carries no `($name)` segment:
/// `array_merge` is fully variadic, so even its FIRST argument is unnamed, where
/// `array_diff` and `array_intersect` do name theirs and leave only the tail unnamed.
/// The name is NOT always `array` either — `array_combine` names both of its slots,
/// `array_fill_keys` names its first `$keys`, and the `iterator_*` pair says `$iterator`.
/// Every entry was measured against `php -n` 8.5.6, not inferred from the signature.
const EVAL_ARRAY_ARG_SITES: &[(&str, usize, Option<&str>, EvalArrayArgKind)] = &[
    ("array_chunk", 0, Some("array"), EvalArrayArgKind::Array),
    ("array_column", 0, Some("array"), EvalArrayArgKind::Array),
    ("array_combine", 0, Some("keys"), EvalArrayArgKind::Array),
    ("array_combine", 1, Some("values"), EvalArrayArgKind::Array),
    ("array_count_values", 0, Some("array"), EvalArrayArgKind::Array),
    ("array_diff", 0, Some("array"), EvalArrayArgKind::Array),
    ("array_diff", 1, None, EvalArrayArgKind::Array),
    ("array_diff_key", 0, Some("array"), EvalArrayArgKind::Array),
    ("array_diff_key", 1, None, EvalArrayArgKind::Array),
    ("array_fill_keys", 0, Some("keys"), EvalArrayArgKind::Array),
    ("array_filter", 0, Some("array"), EvalArrayArgKind::Array),
    ("array_flip", 0, Some("array"), EvalArrayArgKind::Array),
    ("array_intersect", 0, Some("array"), EvalArrayArgKind::Array),
    ("array_intersect", 1, None, EvalArrayArgKind::Array),
    ("array_intersect_key", 0, Some("array"), EvalArrayArgKind::Array),
    ("array_intersect_key", 1, None, EvalArrayArgKind::Array),
    // php's ONLY member of this family whose array is not argument #1.
    ("array_key_exists", 1, Some("array"), EvalArrayArgKind::Array),
    ("array_keys", 0, Some("array"), EvalArrayArgKind::Array),
    ("array_map", 1, Some("array"), EvalArrayArgKind::Array),
    ("array_merge", 0, None, EvalArrayArgKind::Array),
    ("array_merge", 1, None, EvalArrayArgKind::Array),
    ("array_pad", 0, Some("array"), EvalArrayArgKind::Array),
    ("array_product", 0, Some("array"), EvalArrayArgKind::Array),
    ("array_rand", 0, Some("array"), EvalArrayArgKind::Array),
    // Argument #1 is checked BEFORE the callable, measured: a nonexistent callback name
    // still reports the array's type.
    ("array_reduce", 0, Some("array"), EvalArrayArgKind::Array),
    ("array_reverse", 0, Some("array"), EvalArrayArgKind::Array),
    ("array_search", 1, Some("haystack"), EvalArrayArgKind::Array),
    ("array_slice", 0, Some("array"), EvalArrayArgKind::Array),
    ("array_sum", 0, Some("array"), EvalArrayArgKind::Array),
    ("array_unique", 0, Some("array"), EvalArrayArgKind::Array),
    ("array_values", 0, Some("array"), EvalArrayArgKind::Array),
    ("current", 0, Some("array"), EvalArrayArgKind::ArrayOrObject),
    ("implode", 1, Some("array"), EvalArrayArgKind::NullableArray),
    ("in_array", 1, Some("haystack"), EvalArrayArgKind::Array),
    (
        "iterator_count",
        0,
        Some("iterator"),
        EvalArrayArgKind::TraversableOrArray,
    ),
    (
        "iterator_to_array",
        0,
        Some("iterator"),
        EvalArrayArgKind::TraversableOrArray,
    ),
    ("key", 0, Some("array"), EvalArrayArgKind::ArrayOrObject),
];

/// Returns PHP's `TypeError` spelling for one runtime value's type.
///
/// These are PHP's DEBUG type names, not `gettype()`'s legacy ones: a bool prints as its
/// own literal (`false` / `true`) rather than `bool`, an object prints its CLASS name
/// rather than `object`, and null prints lowercase — all measured against `php -n` 8.5.6.
///
/// A class DECLARED INSIDE the same eval has no runtime class cell to read, so its name
/// comes from the eval context first; reading the runtime cell alone reports the backing
/// `stdClass` and would name the wrong class.
pub(in crate::interpreter) fn eval_argument_type_name(
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let tag = values.type_tag(value)?;
    if tag == EVAL_TAG_OBJECT {
        let identity = values.object_identity(value)?;
        if let Some(class) = context.dynamic_object_class(identity) {
            return Ok(class.name().to_string());
        }
        let class_name = values.object_class_name(value)?;
        let bytes = values.string_bytes(class_name);
        values.release(class_name)?;
        let class_name = String::from_utf8(bytes?).map_err(|_| EvalStatus::RuntimeFatal)?;
        return Ok(class_name.trim_start_matches('\\').to_string());
    }
    let name = match tag {
        EVAL_TAG_INT => "int",
        EVAL_TAG_FLOAT => "float",
        EVAL_TAG_STRING => "string",
        // php prints the bool's own literal, so the value has to be read, not just tagged.
        EVAL_TAG_BOOL => {
            if values.truthy(value)? {
                "true"
            } else {
                "false"
            }
        }
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => "array",
        EVAL_TAG_RESOURCE => "resource",
        _ => "null",
    };
    Ok(name.to_string())
}

/// Composes PHP's "must be of type" `TypeError` wording for one argument slot.
///
/// `argument` is ONE-based, as PHP numbers it in the message.
fn eval_array_arg_type_error_message(
    function: &str,
    argument: usize,
    param: Option<&str>,
    expected: &str,
    actual: &str,
) -> String {
    match param {
        Some(param) => format!(
            "{function}(): Argument #{argument} (${param}) must be of type {expected}, {actual} given"
        ),
        None => {
            format!("{function}(): Argument #{argument} must be of type {expected}, {actual} given")
        }
    }
}

/// Throws PHP's `TypeError` unless one argument holds an array.
pub(in crate::interpreter) fn eval_expect_array_arg(
    value: RuntimeCellHandle,
    function: &str,
    argument: usize,
    param: Option<&str>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    eval_expect_declared_array_arg(
        value,
        function,
        argument,
        param,
        EvalArrayArgKind::Array,
        context,
        values,
    )
}

/// Returns whether one object value satisfies an interface, eval declarations included.
///
/// A class declared INSIDE the eval has no runtime class cell to interrogate, so the
/// eval context answers first and the runtime cell is only the fallback — the same split
/// `count()` already needs for `Countable`.
fn eval_object_implements(
    value: RuntimeCellHandle,
    interface: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    dynamic_object_is_a(value, interface, false, context, values)?
        .map_or_else(|| values.object_is_a(value, interface, false), Ok)
}

/// Throws PHP's `TypeError` unless one argument satisfies its DECLARED kind.
fn eval_expect_declared_array_arg(
    value: RuntimeCellHandle,
    function: &str,
    argument: usize,
    param: Option<&str>,
    kind: EvalArrayArgKind,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let tag = values.type_tag(value)?;
    if matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Ok(());
    }
    let accepted = match kind {
        EvalArrayArgKind::Array => false,
        // php's `?array` lets a literal null through ZPP; the caller rejects it after.
        EvalArrayArgKind::NullableArray => tag == EVAL_TAG_NULL,
        EvalArrayArgKind::ArrayOrObject => tag == EVAL_TAG_OBJECT,
        EvalArrayArgKind::TraversableOrArray => {
            tag == EVAL_TAG_OBJECT && eval_object_implements(value, "Traversable", context, values)?
        }
    };
    if accepted {
        return Ok(());
    }
    let actual = eval_argument_type_name(value, context, values)?;
    eval_throw_type_error(
        &eval_array_arg_type_error_message(function, argument, param, kind.expected(), &actual),
        context,
        values,
    )
}

/// Validates every table-declared array argument of one builtin against evaluated cells.
///
/// Argument slots the caller did not supply are skipped: PHP reports a missing REQUIRED
/// argument as an `ArgumentCountError`, which is not this family's concern.
pub(in crate::interpreter) fn eval_check_array_args(
    function: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for &(name, index, param, kind) in EVAL_ARRAY_ARG_SITES {
        if name != function {
            continue;
        }
        let Some(&value) = evaluated_args.get(index) else {
            continue;
        };
        eval_expect_declared_array_arg(value, function, index + 1, param, kind, context, values)?;
    }
    Ok(())
}

/// Throws PHP's `TypeError` for either of `implode()`'s two badly typed arguments.
///
/// `implode()` needs its own entry point because PHP checks it in three measured stages
/// and the FIRST one is not about an array at all:
///
/// 1. `$separator` must be a string. Only a value with no string form fails — an array
///    or a non-`Stringable` object — so `implode(false, ["a", "b"])` is `"ab"`, not a
///    throw. That legacy-looking spelling is just the normal signature with a coerced
///    separator; PHP 8 removed the genuinely reversed one.
/// 2. `$array` runs the shared table check, which words the declared type `?array`.
/// 3. `null` survives stage 2 because the declaration is NULLABLE, and PHP then rejects
///    it with a completely different sentence naming BOTH parameters.
///
/// Stage 1 must run first, or `implode([], ",")` would report argument #2.
pub(in crate::interpreter) fn eval_expect_implode_args(
    separator: RuntimeCellHandle,
    array: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let separator_tag = values.type_tag(separator)?;
    let separator_is_stringable = match separator_tag {
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => false,
        EVAL_TAG_OBJECT => eval_object_implements(separator, "Stringable", context, values)?,
        _ => true,
    };
    if !separator_is_stringable {
        let actual = eval_argument_type_name(separator, context, values)?;
        return eval_throw_type_error(
            &eval_array_arg_type_error_message(
                "implode",
                1,
                Some("separator"),
                "string",
                &actual,
            ),
            context,
            values,
        );
    }
    eval_check_array_args("implode", &[separator, array], context, values)?;
    if values.type_tag(array)? == EVAL_TAG_NULL {
        return eval_throw_type_error(
            "implode(): If argument #1 ($separator) is of type string, \
             argument #2 ($array) must be of type array, null given",
            context,
            values,
        );
    }
    Ok(())
}

/// Throws PHP's `TypeError` unless `count()`'s argument is an array.
///
/// `count()` is the family's odd member twice over: it expects `Countable|array`, and a
/// `Countable` OBJECT is accepted. The caller dispatches `Countable::count()` first, so
/// reaching here means the value is neither — and a plain object still names its class.
pub(in crate::interpreter) fn eval_expect_countable_arg(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if matches!(values.type_tag(value)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Ok(());
    }
    let actual = eval_argument_type_name(value, context, values)?;
    eval_throw_type_error(
        &eval_array_arg_type_error_message("count", 1, Some("value"), "Countable|array", &actual),
        context,
        values,
    )
}

/// Throws PHP's `TypeError` unless a by-reference ordering builtin received an array.
///
/// The whole ordering family shares ONE wording — `sort`, `rsort`, `asort`, `arsort`,
/// `ksort`, `krsort`, `natsort`, `natcasesort`, `shuffle`, `usort`, `uasort` and `uksort`
/// all name their first parameter `$array`, measured against `php -n` 8.5.6.
pub(in crate::interpreter) fn eval_expect_sort_array_arg(
    value: RuntimeCellHandle,
    function: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    eval_expect_array_arg(value, function, 1, Some("array"), context, values)
}
