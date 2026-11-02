//! Purpose:
//! Injects target-independent elephc-PHP `array_merge()` helpers for gradual or associative
//! operands that the indexed-slot runtime helpers cannot represent.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness before type checking.
//!
//! Key details:
//! - The native lowering copies 8- or 16-byte slots out of INDEXED arrays. None of the six
//!   `__rt_array_merge*` helpers walks a hash, so none of them can honour PHP's key rules.
//! - Those rules are what makes the obvious shortcut wrong: `array_merge` renumbers integer keys
//!   but PRESERVES string keys, later winning. Reusing the gradual `array_chunk` trick
//!   (`array_merge(array_values($a), array_values($b))`) would silently drop every string key.
//! - Writing the rule in elephc-PHP needs no new runtime helper: `foreach` over a gradual value,
//!   `is_int` on the key, then an append or a keyed store. Fixed-arity wrappers cover common
//!   three-, five-, and six-array calls. Spread wrappers receive the outer array as an
//!   ordinary argument and flatten it themselves, avoiding the compiler's currently lossy
//!   variadic-parameter materialization. Concrete compatible packed arrays keep the native path.
//! - Verified against `php -n` over 196 array pairs (empty, list, sparse integer keys, string keys,
//!   colliding keys, numeric-string keys, nested arrays) in both spellings of the integer arm.

use crate::parser::ast::Program;
use crate::optimize::reachability::PreludeInventory;

/// Reserved function name used for gradual `array_merge()` calls.
pub(crate) const GRADUAL_ARRAY_MERGE_NAME: &str = "__elephc_array_merge_gradual";

/// Reserved helper name for gradual three-argument `array_merge()` calls.
pub(crate) const GRADUAL_ARRAY_MERGE_THREE_NAME: &str = "__elephc_array_merge_gradual_three";

/// Reserved helper name for gradual five-argument `array_merge()` calls.
pub(crate) const GRADUAL_ARRAY_MERGE_FIVE_NAME: &str = "__elephc_array_merge_gradual_five";

/// Reserved helper name for gradual six-argument `array_merge()` calls.
pub(crate) const GRADUAL_ARRAY_MERGE_SIX_NAME: &str = "__elephc_array_merge_gradual_six";

/// Reserved helper name for `array_merge($first, ...$rest)` calls.
pub(crate) const GRADUAL_ARRAY_MERGE_SPREAD_NAME: &str =
    "__elephc_array_merge_gradual_spread";

/// Reserved helper name for `array_merge(...$arrays)` calls.
pub(crate) const GRADUAL_ARRAY_MERGE_ONLY_SPREAD_NAME: &str =
    "__elephc_array_merge_gradual_only_spread";

/// PHP source for the injected gradual `array_merge()` implementation.
pub const ARRAY_MERGE_PRELUDE_SRC: &str = r#"<?php
function __elephc_array_merge_require_array(mixed $value, int $position): void {
    if (!is_array($value)) {
        throw new TypeError("array_merge(): Argument #" . $position . " must be of type array, " . get_debug_type($value) . " given");
    }
}

function __elephc_array_merge_gradual(mixed $first, mixed $second): array {
    __elephc_array_merge_require_array($first, 1);
    __elephc_array_merge_require_array($second, 2);
    $merged = [];
    foreach ($first as $key => $value) {
        if (is_int($key)) {
            $merged[] = $value;
        } else {
            $merged[$key] = $value;
        }
    }
    foreach ($second as $key => $value) {
        if (is_int($key)) {
            $merged[] = $value;
        } else {
            $merged[$key] = $value;
        }
    }
    return $merged;
}

function __elephc_array_merge_gradual_three(mixed $first, mixed $second, mixed $third): array {
    __elephc_array_merge_require_array($first, 1);
    __elephc_array_merge_require_array($second, 2);
    __elephc_array_merge_require_array($third, 3);
    $merged = [];
    foreach ($first as $key => $value) {
        if (is_int($key)) { $merged[] = $value; } else { $merged[$key] = $value; }
    }
    foreach ($second as $key => $value) {
        if (is_int($key)) { $merged[] = $value; } else { $merged[$key] = $value; }
    }
    foreach ($third as $key => $value) {
        if (is_int($key)) { $merged[] = $value; } else { $merged[$key] = $value; }
    }
    return $merged;
}

function __elephc_array_merge_gradual_five(mixed $first, mixed $second, mixed $third, mixed $fourth, mixed $fifth): array {
    __elephc_array_merge_require_array($first, 1);
    __elephc_array_merge_require_array($second, 2);
    __elephc_array_merge_require_array($third, 3);
    __elephc_array_merge_require_array($fourth, 4);
    __elephc_array_merge_require_array($fifth, 5);
    $merged = [];
    foreach ($first as $key => $value) {
        if (is_int($key)) { $merged[] = $value; } else { $merged[$key] = $value; }
    }
    foreach ($second as $key => $value) {
        if (is_int($key)) { $merged[] = $value; } else { $merged[$key] = $value; }
    }
    foreach ($third as $key => $value) {
        if (is_int($key)) { $merged[] = $value; } else { $merged[$key] = $value; }
    }
    foreach ($fourth as $key => $value) {
        if (is_int($key)) { $merged[] = $value; } else { $merged[$key] = $value; }
    }
    foreach ($fifth as $key => $value) {
        if (is_int($key)) { $merged[] = $value; } else { $merged[$key] = $value; }
    }
    return $merged;
}

function __elephc_array_merge_gradual_six(mixed $first, mixed $second, mixed $third, mixed $fourth, mixed $fifth, mixed $sixth): array {
    __elephc_array_merge_require_array($first, 1);
    __elephc_array_merge_require_array($second, 2);
    __elephc_array_merge_require_array($third, 3);
    __elephc_array_merge_require_array($fourth, 4);
    __elephc_array_merge_require_array($fifth, 5);
    __elephc_array_merge_require_array($sixth, 6);
    $merged = [];
    foreach ($first as $key => $value) {
        if (is_int($key)) { $merged[] = $value; } else { $merged[$key] = $value; }
    }
    foreach ($second as $key => $value) {
        if (is_int($key)) { $merged[] = $value; } else { $merged[$key] = $value; }
    }
    foreach ($third as $key => $value) {
        if (is_int($key)) { $merged[] = $value; } else { $merged[$key] = $value; }
    }
    foreach ($fourth as $key => $value) {
        if (is_int($key)) { $merged[] = $value; } else { $merged[$key] = $value; }
    }
    foreach ($fifth as $key => $value) {
        if (is_int($key)) { $merged[] = $value; } else { $merged[$key] = $value; }
    }
    foreach ($sixth as $key => $value) {
        if (is_int($key)) { $merged[] = $value; } else { $merged[$key] = $value; }
    }
    return $merged;
}

function __elephc_array_merge_gradual_spread(mixed $first, mixed $rest): array {
    __elephc_array_merge_require_array($first, 1);
    $position = 1;
    $merged = [];
    foreach ($first as $key => $value) {
        if (is_int($key)) { $merged[] = $value; } else { $merged[$key] = $value; }
    }
    foreach ($rest as $array) {
        $position++;
        __elephc_array_merge_require_array($array, $position);
        foreach ($array as $key => $value) {
            if (is_int($key)) { $merged[] = $value; } else { $merged[$key] = $value; }
        }
    }
    return $merged;
}

function __elephc_array_merge_gradual_only_spread(mixed $arrays): array {
    $position = 0;
    $merged = [];
    foreach ($arrays as $array) {
        $position++;
        __elephc_array_merge_require_array($array, $position);
        foreach ($array as $key => $value) {
            if (is_int($key)) { $merged[] = $value; } else { $merged[$key] = $value; }
        }
    }
    return $merged;
}
"#;

/// Prepends the gradual `array_merge()` prelude when the program references `array_merge`.
///
/// The reference test is the PHP name rather than the helper's, because the decision to route a
/// call here is made from inferred operand types during EIR lowering — long after injection.
/// `crate::filter_var_prelude` is injected on the same principle.
pub fn inject_if_used(program: Program, inventory: &mut PreludeInventory) -> Program {
    if !crate::ast_usage::collect(&program).references("array_merge") {
        return program;
    }
    let tokens =
        crate::lexer::tokenize(ARRAY_MERGE_PRELUDE_SRC).expect("array_merge prelude must tokenize");
    let mut combined = crate::parser::parse(&tokens).expect("array_merge prelude must parse");
    inventory.record_program("array_merge", &combined);
    combined.extend(program);
    combined
}
