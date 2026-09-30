//! Purpose:
//! The argument-unwrap contract of the array-taking builtin family: which
//! positional arguments accept an `array|false` union, and the shared runtime
//! function that throws php's TypeError when a runtime `false` arrives.
//!
//! Called from:
//! - `crate::ir_lower::expr::array_builtin_args` when wrapping union arguments.
//!
//! Key details:
//! - Only POSITIONAL, by-value arguments belong here: the by-reference family
//!   (sort, array_push, array_pop…) receives the caller's storage and needs its
//!   own write-back treatment.
//! - Each entry's check hook must accept the union through
//!   `array_or_false_member`, or the pair is unreachable.
//! - Each entry is `(builtin, zero-based argument index, php's parameter name)`;
//!   `None` marks a variadic tail slot php's TypeError leaves nameless.

use crate::ir::RuntimeFnId;

/// The one runtime function every entry below shares: unbox the union, and for
/// a runtime `false` throw php's TypeError with the message composed at the
/// call site. Declared beside the table so the family's thrower and the slots
/// it guards cannot drift apart.
pub(crate) const EXPECT_ARRAY_ARG: RuntimeFnId = RuntimeFnId::ExpectArrayArg;

/// The array-taking builtin arguments the lowering unboxes when an
/// `array|false` union flows in, with php's own parameter naming for the
/// TypeError a runtime `false` produces.
pub(crate) const ARRAY_OR_FALSE_ARG_SITES: &[(&str, usize, Option<&str>)] = &[
    ("array_column", 0, Some("array")),
    ("array_count_values", 0, Some("array")),
    ("array_diff", 0, Some("array")),
    ("array_diff", 1, None),
    ("array_diff_key", 0, Some("array")),
    ("array_filter", 0, Some("array")),
    ("array_flip", 0, Some("array")),
    ("array_intersect", 0, Some("array")),
    ("array_intersect", 1, None),
    ("array_intersect_key", 0, Some("array")),
    ("array_keys", 0, Some("array")),
    ("array_map", 1, Some("array")),
    // `array_merge(array ...$arrays)` is fully variadic: even its FIRST argument has no
    // parameter name in php's TypeError, where `array_diff`/`array_intersect` name theirs.
    ("array_merge", 0, None),
    ("array_merge", 1, None),
    ("array_pad", 0, Some("array")),
    ("array_product", 0, Some("array")),
    ("array_rand", 0, Some("array")),
    ("array_reverse", 0, Some("array")),
    ("array_search", 1, Some("haystack")),
    ("array_slice", 0, Some("array")),
    ("array_sum", 0, Some("array")),
    ("array_unique", 0, Some("array")),
    ("array_values", 0, Some("array")),
    ("in_array", 1, Some("haystack")),
];
