//! Purpose:
//! Supplies narrow elephc-PHP implementations for builtin call shapes whose native EIR backend
//! is not available yet.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness before type checking.
//!
//! Key details:
//! - Only the exact supported arities are rewritten, leaving richer builtin forms on their native
//!   diagnostic path instead of silently weakening their PHP semantics.

use crate::parser::ast::Program;

/// Reserved helper used for two-argument `levenshtein()` calls.
pub(crate) const LEVENSHTEIN_TWO_ARG_NAME: &str = "__elephc_levenshtein_two_arg";




/// Reserved helper used when `array_splice()` receives a gradual replacement value.
pub(crate) const ARRAY_SPLICE_MIXED_REPLACEMENT_NAME: &str =
    "__elephc_array_splice_mixed_replacement";

/// Reserved helper used for `array_unshift()` with one trailing positional spread.
pub(crate) const ARRAY_UNSHIFT_TRAILING_SPREAD_NAME: &str =
    "__elephc_array_unshift_trailing_spread";

/// Reserved helper used for supported integer-only `pack()` formats.
pub(crate) const PACK_INTEGER_NAME: &str = "__elephc_pack_integer";

/// Reserved helper used for `random_bytes()`.
pub(crate) const RANDOM_BYTES_NAME: &str = "__elephc_random_bytes";

/// Reserved helper used for one-argument `sort()` calls on gradual indexed arrays.
pub(crate) const SORT_MIXED_NAME: &str = "__elephc_sort_mixed";

/// Reserved helper used for one-argument `rsort()` calls on gradual indexed arrays.
pub(crate) const RSORT_MIXED_NAME: &str = "__elephc_rsort_mixed";

/// Reserved helper the mixed sort hands an all-scalar list to. Its own `sort()`/`rsort()` is the
/// NATIVE one (`ir_lower::expr::compat_preludes` does not redirect a call made inside it), which
/// orders scalars with the runtime's PHP comparison.
pub(crate) const SORT_SCALAR_VALUES_NAME: &str = "__elephc_sort_scalar_values";

/// Reserved helper used for one-argument `array_unique()` calls on gradual associative arrays.
pub(crate) const ARRAY_UNIQUE_ASSOC_MIXED_NAME: &str = "__elephc_array_unique_assoc_mixed";

/// Reserved helper used for two-argument `array_diff()` calls.
pub(crate) const ARRAY_DIFF_STRING_NAME: &str = "__elephc_array_diff_string";

/// Reserved helper used for two-argument `array_intersect()` calls.
pub(crate) const ARRAY_INTERSECT_NAME: &str = "__elephc_array_intersect";

/// Reserved helper used for one-argument `array_reverse()` calls on gradual arrays.
pub(crate) const ARRAY_REVERSE_GRADUAL_NAME: &str = "__elephc_array_reverse_gradual";

/// Reserved helper used for two-argument `array_chunk()` calls on gradual or hash arrays.
pub(crate) const ARRAY_CHUNK_GRADUAL_NAME: &str = "__elephc_array_chunk_gradual";

/// Reserved helper used for the three-argument gradual `array_chunk()`.
pub(crate) const ARRAY_CHUNK_GRADUAL_FLAGGED_NAME: &str = "__elephc_array_chunk_gradual_flagged";

/// Reserved helper used for `array_rand()` over a gradual or hash array.
pub(crate) const ARRAY_RAND_GRADUAL_NAME: &str = "__elephc_array_rand_gradual";

/// Reserved helper used for the three-argument `array_column()`, which re-keys its result.
pub(crate) const ARRAY_COLUMN_INDEXED_NAME: &str = "__elephc_array_column_indexed";

/// Reserved helper used for `array_combine()` across generic array layouts.
pub(crate) const ARRAY_COMBINE_GRADUAL_NAME: &str = "__elephc_array_combine_gradual";

/// Reserved helper used for `array_search()` calls on gradual indexed arrays.
pub(crate) const ARRAY_SEARCH_MIXED_NAME: &str = "__elephc_array_search_mixed";

/// Reserved helper used for `http_build_query()` calls; built by `crate::http_build_query_prelude`.
pub(crate) const HTTP_BUILD_QUERY_NAME: &str = "__elephc_http_build_query";

/// Reserved helpers of the closed-world autoload chain (`--closed-world`): `spl_autoload_register()`
/// and its siblings run on a plain PHP list, because the interpreter that normally owns the chain
/// is not linked. See `AUTOLOAD_CHAIN_SRC`.
pub(crate) const AUTOLOAD_REGISTER_NAME: &str = "__elephc_autoload_register";
pub(crate) const AUTOLOAD_UNREGISTER_NAME: &str = "__elephc_autoload_unregister";
pub(crate) const AUTOLOAD_FUNCTIONS_NAME: &str = "__elephc_autoload_functions";
pub(crate) const AUTOLOAD_CALL_NAME: &str = "__elephc_autoload_call";
/// Retry helper for `class_exists()` and its siblings: runs the chain, then asks again.
pub(crate) const AUTOLOAD_RETRY_NAME: &str = "__elephc_autoload_retry";

/// The native autoload chain.
///
/// php keeps the registered loaders in a list the engine walks on a class miss; this is that list
/// and that walk, written once in PHP over a static property. `register` skips a loader already
/// present and honours `prepend`; `unregister` removes the first identical entry; `call` runs the
/// loaders in order until one makes the class exist and refuses to re-enter for a class whose load
/// is already in progress. Identity (`===`) is the comparison: it is exact for closures and
/// `[object, method]` pairs, which is what php's own duplicate check means by "same callable".
const AUTOLOAD_CHAIN_SRC: &str = r#"<?php
final class __ElephcAutoloadChain {
    public static array $loaders = [];
    public static array $loading = [];
}
function __elephc_autoload_register(mixed $callback = null, bool $throw = true, bool $prepend = false): bool {
    // `$throw` is accepted and ignored, as php 8 does.
    $prepend = $prepend && ($throw || true);
    if ($callback === null) {
        return true;
    }
    foreach (__ElephcAutoloadChain::$loaders as $loader) {
        if ($loader === $callback) {
            return true;
        }
    }
    if ($prepend) {
        array_unshift(__ElephcAutoloadChain::$loaders, $callback);
    } else {
        __ElephcAutoloadChain::$loaders[] = $callback;
    }
    return true;
}
function __elephc_autoload_unregister(mixed $callback): bool {
    $kept = [];
    $found = false;
    foreach (__ElephcAutoloadChain::$loaders as $loader) {
        if (!$found && $loader === $callback) {
            $found = true;
        } else {
            $kept[] = $loader;
        }
    }
    __ElephcAutoloadChain::$loaders = $kept;
    return $found;
}
function __elephc_autoload_functions(): array {
    return __ElephcAutoloadChain::$loaders;
}
function __elephc_autoload_call(string $class): void {
    $key = strtolower(ltrim($class, '\\'));
    if (isset(__ElephcAutoloadChain::$loading[$key])) {
        return;
    }
    __ElephcAutoloadChain::$loading[$key] = true;
    try {
        foreach (__ElephcAutoloadChain::$loaders as $loader) {
            $loader($class);
            if (class_exists($class, false) || interface_exists($class, false)
                || trait_exists($class, false) || enum_exists($class, false)) {
                break;
            }
        }
    } finally {
        unset(__ElephcAutoloadChain::$loading[$key]);
    }
}
function __elephc_autoload_retry(string $class, int $kind): bool {
    __elephc_autoload_call($class);
    if ($kind === 0) {
        return class_exists($class);
    }
    if ($kind === 1) {
        return interface_exists($class);
    }
    if ($kind === 2) {
        return trait_exists($class);
    }
    return enum_exists($class);
}
"#;

/// Reserved helper used for `escapeshellarg()` calls.
pub(crate) const ESCAPESHELLARG_NAME: &str = "__elephc_escapeshellarg";

/// Reserved helper used for `cli_set_process_title()` calls outside a native CLI bridge.
pub(crate) const CLI_SET_PROCESS_TITLE_NAME: &str = "__elephc_cli_set_process_title";

/// Reserved helper used when `file_put_contents()` receives array payload pieces.
pub(crate) const FILE_PUT_CONTENTS_ARRAY_NAME: &str = "__elephc_file_put_contents_array";

/// Reserved helper used for value-callback sorting while preserving array keys.
pub(crate) const UASORT_MIXED_NAME: &str = "__elephc_uasort_mixed";

/// Reserved helper used for value-callback sorting with numeric reindexing.
pub(crate) const USORT_MIXED_NAME: &str = "__elephc_usort_mixed";

/// Reserved helper used for key-callback sorting while preserving associations.
pub(crate) const UKSORT_MIXED_NAME: &str = "__elephc_uksort_mixed";

/// Reserved helper used for ordinary ascending sorting while preserving keys.
pub(crate) const ASORT_MIXED_NAME: &str = "__elephc_asort_mixed";

/// Reserved helper used for `str_getcsv()` calls.
pub(crate) const STR_GETCSV_NAME: &str = "__elephc_str_getcsv";

/// Canonical name of PHP's randomizer class supplied by the compatibility prelude.
pub(crate) use crate::randomizer_prelude::RANDOMIZER_CLASS_NAME;

/// Byte-oriented dynamic-programming implementation matching PHP's default edit costs.
const LEVENSHTEIN_TWO_ARG_SRC: &str = r#"<?php
function __elephc_levenshtein_two_arg(string $first, string $second): int {
    $secondLength = strlen($second);
    $previous = [];
    $j = 0;
    while ($j <= $secondLength) {
        $previous[] = $j;
        $j++;
    }
    $firstLength = strlen($first);
    $i = 1;
    while ($i <= $firstLength) {
        $current = [$i];
        $j = 1;
        while ($j <= $secondLength) {
            $insert = $current[$j - 1] + 1;
            $delete = $previous[$j] + 1;
            $replace = $previous[$j - 1] + ($first[$i - 1] === $second[$j - 1] ? 0 : 1);
            $best = $insert;
            if ($delete < $best) {
                $best = $delete;
            }
            if ($replace < $best) {
                $best = $replace;
            }
            $current[] = $best;
            $j++;
        }
        $previous = $current;
        $i++;
    }
    return $previous[$secondLength];
}
"#;

/// Normalizes a gradual `array_splice()` replacement through PHP's ordinary array cast.
const ARRAY_SPLICE_MIXED_REPLACEMENT_SRC: &str = r#"<?php
function __elephc_array_splice_mixed_replacement(array &$array, int $offset, ?int $length = null, mixed $replacement = []): array {
    $normalized = (array) $replacement;
    return array_splice($array, $offset, $length, $normalized);
}
"#;

/// Prepends fixed values followed by one trailing spread while retaining source order.
const ARRAY_UNSHIFT_TRAILING_SPREAD_SRC: &str = r#"<?php
function __elephc_array_unshift_trailing_spread(mixed &$array, array $leading, array $spread): int {
    if (!is_array($array)) {
        throw new TypeError('array_unshift(): Argument #1 ($array) must be of type array');
    }
    $normalized = (array) $array;
    $index = count($spread);
    while ($index > 0) {
        $index--;
        array_unshift($normalized, $spread[$index]);
    }
    $index = count($leading);
    while ($index > 0) {
        $index--;
        array_unshift($normalized, $leading[$index]);
    }
    $array = $normalized;
    return count($normalized);
}
"#;

/// Packs little- and big-endian unsigned integer fields for `V`, `N`, and `n` formats.
const PACK_INTEGER_SRC: &str = r#"<?php
function __elephc_pack_integer(string $format, int ...$values): string {
    $result = '';
    $length = strlen($format);
    $i = 0;
    while ($i < $length) {
        $value = $values[$i];
        $code = $format[$i];
        if ($code === 'V') {
            $result .= chr($value & 255);
            $result .= chr(($value >> 8) & 255);
            $result .= chr(($value >> 16) & 255);
            $result .= chr(($value >> 24) & 255);
        } elseif ($code === 'N') {
            $result .= chr(($value >> 24) & 255);
            $result .= chr(($value >> 16) & 255);
            $result .= chr(($value >> 8) & 255);
            $result .= chr($value & 255);
        } else {
            $result .= chr(($value >> 8) & 255);
            $result .= chr($value & 255);
        }
        $i++;
    }
    return $result;
}
"#;

/// Builds a binary string from Elephc's cryptographic `random_int()` primitive.
const RANDOM_BYTES_SRC: &str = r#"<?php
function __elephc_random_bytes(int $length): string {
    if ($length < 1) {
        throw new \ValueError('random_bytes(): Argument #1 ($length) must be greater than 0');
    }
    $result = '';
    $i = 0;
    while ($i < $length) {
        $result .= chr(random_int(0, 255));
        $i++;
    }
    return $result;
}
"#;

/// Stable in-place value sort using PHP's ordinary comparison semantics.
///
/// Scalar and null elements go to the native `sort()`/`rsort()` of a typed array, which orders
/// them with the runtime's own PHP comparison (`__rt_php_compare_slots`). Only an element the
/// native comparator refuses -- an array or an object -- keeps the quadratic loop below: Twig
/// sorts `get_class_methods()` of every class it reads an attribute from, and the loop made that
/// 1.4M instructions for 28 names.
const SORT_MIXED_SRC: &str = r#"<?php
function __elephc_sort_mixed_direction(mixed &$array, bool $descending): bool {
    $values = array_values($array);
    $scalar = true;
    foreach ($values as $value) {
        if ($value !== null && !is_scalar($value)) {
            $scalar = false;
            break;
        }
    }
    if ($scalar) {
        $array = __elephc_sort_scalar_values($values, $descending);
        return true;
    }
    $array = $values;
    $count = count($array);
    $outer = $count;
    while ($outer > 1) {
        $inner = 0;
        while ($inner + 1 < $outer) {
            $next = $inner + 1;
            $comparison = __elephc_sort_mixed_compare($array[$inner], $array[$next]);
            $swap = $descending ? $comparison < 0 : $comparison > 0;
            if ($swap) {
                $temporary = $array[$inner];
                $array[$inner] = $array[$next];
                $array[$next] = $temporary;
            }
            $inner++;
        }
        $outer--;
    }
    return true;
}
function __elephc_sort_mixed_compare(mixed $current, mixed $candidate): int {
    if (is_array($current) && is_array($candidate)) {
        $currentCount = count($current);
        $candidateCount = count($candidate);
        if ($currentCount !== $candidateCount) {
            return $currentCount < $candidateCount ? -1 : 1;
        }
        foreach ($current as $key => $value) {
            if (!array_key_exists($key, $candidate)) {
                return 1;
            }
            $nested = __elephc_sort_mixed_compare($value, $candidate[$key]);
            if ($nested !== 0) {
                return $nested;
            }
        }
        return 0;
    }
    if (is_string($current) && is_string($candidate)) {
        return strcmp($current, $candidate);
    }
    return $current == $candidate ? 0 : ($current > $candidate ? 1 : -1);
}
function __elephc_sort_scalar_values(array $values, bool $descending): array {
    if ($descending) {
        rsort($values);
    } else {
        sort($values);
    }
    return $values;
}
function __elephc_sort_mixed(mixed &$array): bool {
    return __elephc_sort_mixed_direction($array, false);
}
function __elephc_rsort_mixed(mixed &$array): bool {
    return __elephc_sort_mixed_direction($array, true);
}
"#;

/// Default `SORT_STRING` de-duplication for gradual associative values, preserving first keys.
const ARRAY_UNIQUE_ASSOC_MIXED_SRC: &str = r#"<?php
function __elephc_array_unique_assoc_mixed(array $values): array {
    $seen = ["__elephc_seed" => true];
    unset($seen["__elephc_seed"]);
    $result = ["__elephc_seed" => null];
    unset($result["__elephc_seed"]);
    foreach ($values as $key => $value) {
        $comparison = (string) $value;
        if (!isset($seen[$comparison])) {
            $seen[$comparison] = true;
            $result[$key] = $value;
        }
    }
    return $result;
}
"#;

/// Two-array difference that compares values by PHP string cast and preserves left keys.
const ARRAY_DIFF_STRING_SRC: &str = r#"<?php
function __elephc_array_diff_string(array $left, array $right): array {
    $result = ["__elephc_seed" => null];
    unset($result["__elephc_seed"]);
    foreach ($left as $key => $value) {
        $found = false;
        foreach ($right as $candidate) {
            if ((string) $value === (string) $candidate) {
                $found = true;
                break;
            }
        }
        if (!$found) {
            $result[$key] = $value;
        }
    }
    return $result;
}
"#;

/// Two-array intersection that compares values by PHP string cast and preserves left keys.
const ARRAY_INTERSECT_SRC: &str = r#"<?php
function __elephc_array_intersect(array $left, array $right): mixed {
    $result = [];
    foreach ($left as $key => $value) {
        foreach ($right as $candidate) {
            if ((string) $value === (string) $candidate) {
                if (is_string($key)) {
                    $result[(string) $key] = $value;
                } else {
                    $result[(int) $key] = $value;
                }
                break;
            }
        }
    }
    return $result;
}
"#;

/// Reverses a gradual array, renumbering integer keys while preserving string keys.
const ARRAY_REVERSE_GRADUAL_SRC: &str = r#"<?php
function __elephc_array_reverse_gradual(array $input): array {
    $keys = [];
    $values = [];
    foreach ($input as $key => $value) {
        $keys[] = $key;
        $values[] = $value;
    }
    $result = [];
    $position = count($values);
    while ($position > 0) {
        $position--;
        $key = $keys[$position];
        if (is_string($key)) {
            $result[$key] = $values[$position];
        } else {
            $result[] = $values[$position];
        }
    }
    return $result;
}
"#;

/// Splits any array layout into renumbered chunks, which is PHP's two-argument behavior.
///
/// Written against a materialized list rather than accumulating into a reset `$chunk`, because
/// that shorter shape is MISCOMPILED. Reduced:
///
/// ```php
/// function g(array $in): array {
///     $result = []; $chunk = [];
///     foreach ($in as $v) {
///         $chunk[] = $v;
///         if (count($chunk) === 2) { $result[] = $chunk; $chunk = []; }
///     }
///     if (count($chunk) > 0) { $result[] = $chunk; }
///     return $result;
/// }
/// ```
///
/// The callee returns `array<mixed>` -- boxed elements, because loop-storage stabilization widened
/// `$result` before the loop -- while the CALL SITE is typed `array<array<mixed>>`. The caller then
/// reads each boxed cell as a raw array: `count()` answers the cell's runtime tag (4) instead of
/// the chunk length. `php -n` prints `[1/2][3/4][5]`; elephc prints pointers and then segfaults.
///
/// Binding `$chunk` fresh inside the outer loop and giving `$result` a single append site keeps
/// both sides on the same representation. Do not "simplify" this back without re-checking that
/// reduction.
const ARRAY_CHUNK_GRADUAL_SRC: &str = r#"<?php
function __elephc_array_chunk_gradual(array $input, int $length): array {
    if ($length < 1) {
        throw new ValueError('array_chunk(): Argument #2 ($length) must be greater than 0');
    }
    $values = [];
    foreach ($input as $value) {
        $values[] = $value;
    }
    $total = count($values);
    $result = [];
    $start = 0;
    while ($start < $total) {
        $chunk = [];
        $offset = 0;
        while ($offset < $length && $start + $offset < $total) {
            $chunk[] = $values[$start + $offset];
            $offset++;
        }
        $result[] = $chunk;
        $start += $length;
    }
    return $result;
}
"#;

/// Chunks a generic array layout when `preserve_keys` is only known at run time.
///
/// Separate from the two-argument helper because the shapes differ: that one's chunks are dense
/// arrays and its result type says so, while these carry the source keys and are hashes.
///
/// EVERY write here goes through an explicit key -- `$chunk[$key]` and `$result[$outer]` -- so both
/// levels keep ONE representation. An append (`$result[] =`) next to a keyed write is what made the
/// first attempt at this helper segfault at the first consumer: the caller read boxed cells where
/// the callee had stored raw pointers. The declared result type must be the one this body infers.
const ARRAY_CHUNK_GRADUAL_FLAGGED_SRC: &str = r#"<?php
function __elephc_array_chunk_gradual_flagged(array $input, int $length, $preserveKeys): array {
    if ($length < 1) {
        throw new ValueError('array_chunk(): Argument #2 ($length) must be greater than 0');
    }
    $keys = [];
    $values = [];
    foreach ($input as $key => $value) {
        $keys[] = $key;
        $values[] = $value;
    }
    $total = count($values);
    $result = [];
    $outer = 0;
    $start = 0;
    while ($start < $total) {
        $chunk = [];
        $offset = 0;
        while ($offset < $length && $start + $offset < $total) {
            $chunkKey = $preserveKeys ? $keys[$start + $offset] : $offset;
            $chunk[$chunkKey] = $values[$start + $offset];
            $offset = $offset + 1;
        }
        $result[$outer] = $chunk;
        $outer = $outer + 1;
        $start = $start + $length;
    }
    return $result;
}
"#;

/// Picks one random KEY out of a generic array layout.
///
/// `__rt_array_rand` walks the fixed-size slots of a dense indexed array, so a hash or a gradual
/// value has no native path and the key it would answer is a string as often as an integer.
/// Collecting the keys into a dense list first puts the random pick back on the native helper,
/// and the answer is the key itself, which is what php returns.
const ARRAY_RAND_GRADUAL_SRC: &str = r#"<?php
function __elephc_array_rand_gradual(array $input) {
    $keys = [];
    foreach ($input as $key => $value) {
        $keys[] = $key;
    }
    if (0 === count($keys)) {
        throw new ValueError('array_rand(): Argument #1 ($array) cannot be empty');
    }
    return $keys[array_rand($keys)];
}
"#;

/// Extracts one column and re-keys the result, which is php's three-argument `array_column()`.
///
/// The backend has no lowering for the `$index_key` form: it produces a HASH whose keys come from
/// the data, which a dense indexed result cannot express. Written here in PHP instead, with ONE
/// `$result[$key] = $value;` write site so the whole result keeps a single representation -- the
/// same rule the gradual `array_chunk()` helper documents, and the one whose violation segfaults at
/// the first consumer rather than failing to build.
///
/// `$next` reproduces php's own append key: a row with no usable index key takes the next integer,
/// and an integer index key pushes that counter past itself, exactly as `$result[] =` would.
const ARRAY_COLUMN_INDEXED_SRC: &str = r#"<?php
function __elephc_array_column_indexed(array $input, $column, $index): array {
    $result = [];
    $next = 0;
    foreach ($input as $row) {
        $row = (array) $row;
        if (null === $column) {
            $value = $row;
        } elseif (array_key_exists($column, $row)) {
            $value = $row[$column];
        } else {
            continue;
        }
        if (null !== $index && array_key_exists($index, $row)) {
            $key = $row[$index];
            if (is_int($key) && $key >= $next) {
                $next = $key + 1;
            }
        } else {
            $key = $next;
            $next = $next + 1;
        }
        $result[$key] = $value;
    }
    return $result;
}
"#;

/// Combines generic array layouts in iteration order after validating equal cardinality.
const ARRAY_COMBINE_GRADUAL_SRC: &str = r#"<?php
function __elephc_array_combine_gradual(array $keys, array $values): array {
    if (count($keys) !== count($values)) {
        throw new ValueError('array_combine(): Argument #1 ($keys) and argument #2 ($values) must have the same number of elements');
    }
    $orderedValues = [];
    foreach ($values as $value) {
        $orderedValues[] = $value;
    }
    $result = [];
    $position = 0;
    foreach ($keys as $key) {
        if (!is_int($key) && !is_string($key)) {
            $key = (string) $key;
        }
        $result[$key] = $orderedValues[$position];
        $position++;
    }
    return $result;
}
"#;

/// Searches a gradual packed list using PHP's loose or strict equality semantics.
const ARRAY_SEARCH_MIXED_SRC: &str = r#"<?php
function __elephc_array_search_mixed(mixed $needle, array $haystack, bool $strict = false): int|false {
    foreach ($haystack as $key => $value) {
        if ($strict ? $needle === $value : $needle == $value) {
            return $key;
        }
    }
    return false;
}
"#;

/// POSIX shell argument quoting compatible with PHP on the supported Unix targets.
const ESCAPESHELLARG_SRC: &str = r#"<?php
function __elephc_escapeshellarg(string $argument): string {
    $result = "'";
    $length = strlen($argument);
    $position = 0;
    while ($position < $length) {
        $character = $argument[$position];
        $result .= $character === "'" ? "'\\''" : $character;
        $position++;
    }
    return $result."'";
}
"#;

/// Reports that process-title mutation is unavailable without changing observable process state.
const CLI_SET_PROCESS_TITLE_SRC: &str = r#"<?php
function __elephc_cli_set_process_title(string $title): bool {
    return false;
}
"#;

/// Concatenates array pieces before delegating to the native file writer, as PHP does.
const FILE_PUT_CONTENTS_ARRAY_SRC: &str = r#"<?php
function __elephc_file_put_contents_array(string $filename, array $data, int $flags = 0, mixed $context = null): int|false {
    $contents = '';
    foreach ($data as $piece) {
        $contents .= (string) $piece;
    }
    return file_put_contents($filename, $contents, $flags, $context);
}
"#;

/// Stable callback sorts for arrays with gradual element layouts, preserving PHP's key rules.
const CALLBACK_SORT_MIXED_SRC: &str = r#"<?php
function __elephc_uasort_mixed(array &$values, callable $callback): bool {
    $keys = [];
    foreach ($values as $key => $value) {
        $keys[] = $key;
    }
    $count = count($keys);
    $outer = $count;
    while ($outer > 1) {
        $inner = 0;
        while ($inner + 1 < $outer) {
            $next = $inner + 1;
            if ($callback($values[$keys[$inner]], $values[$keys[$next]]) > 0) {
                $temporary = $keys[$inner];
                $keys[$inner] = $keys[$next];
                $keys[$next] = $temporary;
            }
            $inner++;
        }
        $outer--;
    }
    $sorted = ["__elephc_seed" => null];
    unset($sorted["__elephc_seed"]);
    foreach ($keys as $key) {
        $sorted[$key] = $values[$key];
    }
    $values = $sorted;
    return true;
}

function __elephc_usort_mixed(array &$values, callable $callback): bool {
    $values = array_values($values);
    $count = count($values);
    $outer = $count;
    while ($outer > 1) {
        $inner = 0;
        while ($inner + 1 < $outer) {
            $next = $inner + 1;
            if ($callback($values[$inner], $values[$next]) > 0) {
                $temporary = $values[$inner];
                $values[$inner] = $values[$next];
                $values[$next] = $temporary;
            }
            $inner++;
        }
        $outer--;
    }
    return true;
}

function __elephc_uksort_mixed(array &$values, callable $callback): bool {
    $keys = [];
    foreach ($values as $key => $value) {
        $keys[] = $key;
    }
    $count = count($keys);
    $outer = $count;
    while ($outer > 1) {
        $inner = 0;
        while ($inner + 1 < $outer) {
            $next = $inner + 1;
            if ($callback($keys[$inner], $keys[$next]) > 0) {
                $temporary = $keys[$inner];
                $keys[$inner] = $keys[$next];
                $keys[$next] = $temporary;
            }
            $inner++;
        }
        $outer--;
    }
    $sorted = ["__elephc_seed" => null];
    unset($sorted["__elephc_seed"]);
    foreach ($keys as $key) {
        $sorted[$key] = $values[$key];
    }
    $values = $sorted;
    return true;
}

function __elephc_asort_mixed(array &$values): bool {
    return __elephc_uasort_mixed($values, static fn(mixed $left, mixed $right): int => $left <=> $right);
}
"#;

/// Parses one CSV record while preserving separators and doubled enclosures inside quoted fields.
const STR_GETCSV_SRC: &str = r#"<?php
function __elephc_str_getcsv(string $string, string $separator = ',', string $enclosure = '"', string $escape = "\\"): array {
    if ($string === '') {
        return [null];
    }
    $result = [];
    $field = '';
    $length = strlen($string);
    $index = 0;
    $quoted = false;
    while ($index < $length) {
        $character = $string[$index];
        if ($quoted) {
            if ($escape !== '' && $character === $escape && $index + 1 < $length) {
                $field .= $character;
                $index++;
                $field .= $string[$index];
            } elseif ($character === $enclosure) {
                if ($index + 1 < $length && $string[$index + 1] === $enclosure) {
                    $field .= $enclosure;
                    $index++;
                } else {
                    $quoted = false;
                }
            } else {
                $field .= $character;
            }
        } elseif ($character === $separator) {
            $result[] = $field;
            $field = '';
        } elseif ($character === $enclosure && $field === '') {
            $quoted = true;
        } else {
            $field .= $character;
        }
        $index++;
    }
    $result[] = $field;
    return $result;
}
"#;

/// PHP-visible wrapper for the supported `levenshtein()` call shape.
const LEVENSHTEIN_WRAPPER_SRC: &str = r#"<?php
function levenshtein(string $first, string $second): int { return __elephc_levenshtein_two_arg($first, $second); }
"#;

/// PHP-visible wrapper for `random_bytes()`.
const RANDOM_BYTES_WRAPPER_SRC: &str = r#"<?php
function random_bytes(int $length): string { return __elephc_random_bytes($length); }
"#;

/// PHP-visible wrapper for `escapeshellarg()`.
const ESCAPESHELLARG_WRAPPER_SRC: &str = r#"<?php
function escapeshellarg(string $argument): string { return __elephc_escapeshellarg($argument); }
"#;

/// PHP-visible wrappers for the process-title aliases.
const CLI_SET_PROCESS_TITLE_WRAPPER_SRC: &str = r#"<?php
function cli_set_process_title(string $title): bool { return __elephc_cli_set_process_title($title); }
function setproctitle(string $title): bool { return __elephc_cli_set_process_title($title); }
"#;

/// PHP-visible wrapper for `str_getcsv()`.
const STR_GETCSV_WRAPPER_SRC: &str = r#"<?php
function str_getcsv(string $string, string $separator = ',', string $enclosure = '"', string $escape = "\\"): array {
    return __elephc_str_getcsv($string, $separator, $enclosure, $escape);
}
"#;

/// Prepends each narrow helper whose corresponding PHP builtin is referenced.
/// Reachability group every backend-gap helper is recorded under.
pub const BACKEND_GAP_GROUP: &str = "backend_gap";

pub fn inject_if_used(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    let usage = crate::ast_usage::collect(&program);
    let mut sources = Vec::new();
    let inject_randomizer = usage.constructs(RANDOMIZER_CLASS_NAME)
        && !program.iter().any(|stmt| {
            matches!(
                &stmt.kind,
                crate::parser::ast::StmtKind::ClassDecl { name, .. }
                    if php_class_name_is(name, RANDOMIZER_CLASS_NAME)
            )
        });
    if usage.references("levenshtein") {
        sources.push(LEVENSHTEIN_TWO_ARG_SRC);
        sources.push(LEVENSHTEIN_WRAPPER_SRC);
    }
    if usage.references("array_splice") {
        sources.push(ARRAY_SPLICE_MIXED_REPLACEMENT_SRC);
    }
    if usage.references("array_unshift") {
        sources.push(ARRAY_UNSHIFT_TRAILING_SPREAD_SRC);
    }
    if usage.references("pack") {
        sources.push(PACK_INTEGER_SRC);
    }
    if usage.references("random_bytes") {
        sources.push(RANDOM_BYTES_SRC);
        sources.push(RANDOM_BYTES_WRAPPER_SRC);
    }
    if usage.references("sort") || usage.references("rsort") {
        sources.push(SORT_MIXED_SRC);
    }
    if usage.references("array_unique") {
        sources.push(ARRAY_UNIQUE_ASSOC_MIXED_SRC);
    }
    if usage.references("array_diff") {
        sources.push(ARRAY_DIFF_STRING_SRC);
    }
    if usage.references("array_intersect") {
        sources.push(ARRAY_INTERSECT_SRC);
    }
    if usage.references("array_chunk") {
        sources.push(ARRAY_CHUNK_GRADUAL_SRC);
        sources.push(ARRAY_CHUNK_GRADUAL_FLAGGED_SRC);
    }
    if usage.references("array_reverse") {
        sources.push(ARRAY_REVERSE_GRADUAL_SRC);
    }
    if usage.references("array_rand") {
        sources.push(ARRAY_RAND_GRADUAL_SRC);
    }
    if usage.references("array_column") {
        sources.push(ARRAY_COLUMN_INDEXED_SRC);
    }
    if usage.references("array_combine") {
        sources.push(ARRAY_COMBINE_GRADUAL_SRC);
    }
    if usage.references("array_search") {
        sources.push(ARRAY_SEARCH_MIXED_SRC);
    }
    if crate::closed_world::is_enabled()
        && ["spl_autoload_register", "spl_autoload_unregister", "spl_autoload_functions", "spl_autoload_call"]
            .iter()
            .any(|name| usage.references(name))
    {
        sources.push(AUTOLOAD_CHAIN_SRC);
    }
    if usage.references("escapeshellarg") {
        sources.push(ESCAPESHELLARG_SRC);
        sources.push(ESCAPESHELLARG_WRAPPER_SRC);
    }
    if usage.references("cli_set_process_title") || usage.references("setproctitle") {
        sources.push(CLI_SET_PROCESS_TITLE_SRC);
        sources.push(CLI_SET_PROCESS_TITLE_WRAPPER_SRC);
    }
    if usage.references("file_put_contents") {
        sources.push(FILE_PUT_CONTENTS_ARRAY_SRC);
    }
    // `natsort()`/`natcasesort()` over a gradual array sort through `uasort()` in
    // `crate::natsort_gradual_prelude`, so they need its gradual helper as well.
    if usage.references("uasort") || usage.references("usort") || usage.references("uksort") || usage.references("asort")
        || usage.references("natsort") || usage.references("natcasesort")
    {
        sources.push(CALLBACK_SORT_MIXED_SRC);
    }
    let needs_csv_long_forms = usage.references(crate::csv_prelude::FPUTCSV_FULL_NAME)
        || usage.references(crate::csv_prelude::FGETCSV_FULL_NAME);
    if usage.references("str_getcsv") {
        sources.push(STR_GETCSV_SRC);
        sources.push(STR_GETCSV_WRAPPER_SRC);
    } else if usage.references(crate::csv_prelude::FGETCSV_FULL_NAME) {
        // The long `fgetcsv()` parses its record with the `str_getcsv()` helper.
        sources.push(STR_GETCSV_SRC);
    }
    // Natural-order comparison is BUILT rather than parsed, so it never joins `sources`.
    // `natsort()`/`natcasesort()` over a gradual array compare through them too.
    let needs_natural_order = usage.references("strnatcmp")
        || usage.references("strnatcasecmp")
        || usage.references("natsort")
        || usage.references("natcasesort");
    // The tokenizer surface (`token_get_all()`, `token_name()`, `PhpToken`) is not injected here:
    // `crate::tokenizer_prelude` owns it, in the pipeline's early prelude phase and again through
    // the autoload probe.
    // And so is a whole-array read of `$GLOBALS` (psysh's `GlobalVariableEnumerator`).
    let needs_globals_snapshot = crate::globals_snapshot_prelude::needed_by(&program);
    // `array_filter()` no longer needs a helper: the boxed predicate runtime acquires a
    // descriptor from any callback value, a boxed one included, and tests null at run time.
    let needs_array_filter_callback = false;
    // So is `array_map()`'s multi-array form.
    // `ast_usage` records the helper's name only for a call with two or more arrays: the helper
    // brings callable invokers of its own, and a program mapping one array must not carry them.
    let needs_array_map_multi =
        usage.references(crate::array_map_prelude::ARRAY_MAP_MULTI_NAME);
    // So is `array_keys()`'s search form, recorded the same way.
    let needs_array_keys_search =
        usage.references(crate::array_keys_prelude::ARRAY_KEYS_SEARCH_NAME);
    let needs_array_map_gradual =
        usage.references(crate::array_map_prelude::ARRAY_MAP_GRADUAL_NAME);
    // And so is the `array_walk()` family, whose native runtimes answer neither the by-reference
    // callback `array_walk()` is FOR nor a nested array in `array_walk_recursive()`.
    let needs_array_walk =
        usage.references("array_walk") || usage.references("array_walk_recursive");
    // And so is `preg_match_all()` with `$flags`, which the native helper ignores.
    // And so are the array and flag forms of the `preg_replace*()` family, `preg_filter()` and
    // `preg_replace_callback_array()`. The callback `$flags` form finds its matches through the
    // native `preg_match_all()`, which builds every flag form itself.
    let needs_preg_array = usage.references("preg_replace")
        || usage.references("preg_replace_callback")
        || usage.references("preg_filter")
        || usage.references("preg_replace_callback_array");
    // And so are `is_callable()`'s second and third parameters: the backend predicate takes the
    // value alone, and `$syntax_only` asks a different question than "can this be called".
    let needs_is_callable_ext =
        usage.references(crate::is_callable_prelude::IS_CALLABLE_EXT_NAME);
    // And so is `Closure::fromCallable` named at run time (`['Closure', 'fromCallable']`): the
    // run-time static-method table only lists compiled classes, and `Closure` is none.
    let needs_closure_from_callable = crate::closure_from_callable_prelude::named_by(&program);
    // And so is intl's `Locale` default, which Symfony reads once `class_exists(\Locale::class)`.
    let needs_locale = crate::locale_prelude::needed_by(&program);
    // php 8.4's `BcMath\Number` is not injected here: `crate::bcmath_prelude` owns it (with its
    // typed object operators), in the pipeline's early prelude phase.
    // And so are the length / context forms of `fgets()`, `fwrite()`, `rmdir()` and friends.
    let needs_stream_args = crate::stream_args_prelude::HELPERS
        .iter()
        .any(|helper| usage.references(helper));
    // And so is `parse_str()`, which has no backend implementation of any kind: no runtime symbol
    // and no lowering, so a compiled call fell through to the eval bridge, whose array ABI cannot
    // bind the mandatory by-reference `$result`.
    let needs_parse_str = usage.references("parse_str");
    // And so is `strtok()`, which has no backend implementation either; a program declaring its
    // own `strtok` (a polyfill) keeps its definition.
    let needs_strtok = usage.references("strtok")
        && !crate::program_declares::program_declares_anywhere(&program, "strtok");
    // And so is the `proc_open()` family, over the `elephc_proc` bridge. Each member a program
    // declares itself (a polyfill) keeps its definition and is left out.
    let proc_functions: Vec<&str> = crate::proc_prelude::FUNCTIONS
        .iter()
        .copied()
        .filter(|name| {
            usage.references(name)
                && !crate::program_declares::program_declares_anywhere(&program, name)
        })
        .collect();
    let needs_proc = !proc_functions.is_empty();
    // And so is `exec()`'s by-reference form, recorded under the helper's name by `ast_usage`.
    let needs_exec_full = usage.references(crate::exec_prelude::EXEC_FULL_NAME);
    // And so is `stream_select()`, whose native helper reads three non-null packed arrays.
    let needs_stream_select = usage.references(crate::stream_select_prelude::STREAM_SELECT_NAME);
    // And so is a one-argument `min()` / `max()` over a gradual operand.
    let needs_min_max_mixed = usage.references(crate::min_max_prelude::MIN_MAX_MIXED_NAME);
    // And so is a spread into an array literal: the source's type decides the route.
    let needs_spread_source = usage.references(crate::spread_prelude::SPREAD_SOURCE_NAME);
    // And so is `count($value, COUNT_RECURSIVE)` over a receiver that can nest.
    let needs_count_recursive =
        usage.references(crate::count_recursive_prelude::COUNT_RECURSIVE_NAME);
    // And so is a `string` first argument of `abs()` / `ceil()` / `floor()` / `round()`.
    let needs_numeric_string_arg =
        usage.references(crate::numeric_string_arg_prelude::NUMERIC_STRING_ARG_NAME);
    // And so is `array_fill_keys()` over gradual keys.
    let needs_array_fill_keys_mixed =
        usage.references(crate::array_fill_keys_prelude::ARRAY_FILL_KEYS_MIXED_NAME);
    // And so is `addcslashes()`, which has no backend implementation either.
    let needs_addcslashes = usage.references("addcslashes");
    // And so are the ext/standard functions with no other implementation (`uniqid()`, ...).
    let standard_gaps: Vec<&str> = crate::standard_gap_prelude::FUNCTIONS
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| {
            usage.references(name)
                && !crate::program_declares::program_declares_anywhere(&program, name)
        })
        .collect();
    let needs_standard_gaps = !standard_gaps.is_empty();
    // And so is `str_replace()`'s by-reference `$count`, which the native helper does not keep.
    let needs_str_replace_count = usage.references("str_replace");
    // And so is `implode()` over objects or floats, which the native joiners cannot render.
    let needs_implode_values = usage.references("implode") || usage.references("join");
    // And so is `array_sum()` / `array_product()` over anything but a list of ints or floats.
    let needs_array_sum_values =
        usage.references("array_sum") || usage.references("array_product");
    // And so is `array_pad()` over a list the native helper cannot copy.
    let needs_array_pad_values = usage.references("array_pad");
    // And so is a `define()` whose name is computed, which needs the run-time constant table.
    let needs_dynamic_define = usage.dynamic_define;
    // And so is `~` over a string, which `IBitNot` only computes for integers.
    let needs_string_bitnot = crate::string_bitnot_prelude::program_uses_bitnot(&program);
    // And so is `htmlspecialchars()`'s fourth parameter: the backend helper always
    // double-encodes, so `$double_encode = false` is answered by a prelude loop.
    let needs_htmlspecialchars = usage.references("htmlspecialchars");
    // And so are `array_any()` / `array_all()` / `array_find()`, whose native runtime walks only a
    // dense list and passes the predicate no key.
    let needs_array_find = usage.references("array_any")
        || usage.references("array_all")
        || usage.references("array_find");
    // And so is `array_splice()` over a hash or a boxed receiver: the native lowering splices a
    // dense slot array only.
    let needs_array_splice = usage.references("array_splice");
    // And so is `http_build_query()`, whose helper and PHP-visible wrapper are built.
    let needs_http_build_query = usage.references("http_build_query");
    // And so is `version_compare()`, which had no implementation of any kind.
    let needs_version_compare = usage.references("version_compare");
    // And so is `hash_pbkdf2()`, built over the backend's raw `hash_hmac()`.
    let needs_hash_pbkdf2 = usage.references("hash_pbkdf2");
    // And so is a method call on an unknown receiver whose class declares `__call()` but not the
    // method: the dispatch ladder lists only the classes that declare it.
    let needs_magic_call = crate::magic_call_prelude::program_declares_magic_call(&program);
    if sources.is_empty()
        && !needs_csv_long_forms
        && !needs_array_find
        && !needs_array_splice
        && !needs_addcslashes
        && !needs_standard_gaps
        && !needs_strtok
        && !needs_proc
        && !needs_exec_full
        && !needs_stream_select
        && !needs_min_max_mixed
        && !needs_spread_source
        && !needs_count_recursive
        && !needs_numeric_string_arg
        && !needs_array_fill_keys_mixed
        && !needs_str_replace_count
        && !needs_htmlspecialchars
        && !needs_http_build_query
        && !needs_version_compare
        && !needs_hash_pbkdf2
        && !needs_magic_call
        && !inject_randomizer
        && !needs_natural_order
        && !needs_parse_str
        && !needs_array_filter_callback
        && !needs_array_map_multi
        && !needs_array_keys_search
        && !needs_array_map_gradual
        && !needs_array_walk
        && !needs_preg_array
        && !needs_is_callable_ext
        && !needs_closure_from_callable
        && !needs_locale
        && !needs_globals_snapshot
        && !needs_stream_args
        && !needs_implode_values
        && !needs_array_sum_values
        && !needs_array_pad_values
        && !needs_dynamic_define
        && !needs_string_bitnot
    {
        return program;
    }
    let mut combined = Vec::new();
    if needs_string_bitnot {
        let declarations = crate::string_bitnot_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_dynamic_define {
        let declarations = crate::dynamic_define_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_implode_values {
        let declarations = crate::implode_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_array_sum_values {
        let declarations = crate::array_sum_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_array_pad_values {
        let declarations = crate::array_pad_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_csv_long_forms {
        let declarations = crate::csv_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_array_find {
        let declarations = crate::array_find_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_array_splice {
        let declarations = crate::array_splice_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_htmlspecialchars {
        let declarations = crate::htmlspecialchars_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_strtok {
        let declarations = crate::strtok_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_proc {
        let declarations = crate::proc_prelude::declarations(&proc_functions);
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_exec_full {
        let declarations = crate::exec_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_stream_select {
        let declarations = crate::stream_select_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_min_max_mixed {
        let declarations = crate::min_max_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_spread_source {
        let declarations = crate::spread_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_count_recursive {
        let declarations = crate::count_recursive_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_numeric_string_arg {
        let declarations = crate::numeric_string_arg_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_array_fill_keys_mixed {
        let declarations = crate::array_fill_keys_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_addcslashes {
        let declarations = crate::addcslashes_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_standard_gaps {
        let declarations = crate::standard_gap_prelude::declarations(&standard_gaps);
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_str_replace_count {
        let declarations = crate::str_replace_count_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_http_build_query {
        let declarations = crate::http_build_query_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_version_compare {
        let declarations = crate::version_compare_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_magic_call {
        let declarations = crate::magic_call_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_hash_pbkdf2 {
        let declarations = crate::hash_pbkdf2_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_natural_order {
        let declarations = crate::strnatcmp_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_parse_str {
        let declarations = crate::parse_str_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_array_filter_callback {
        let declarations = crate::array_filter_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_array_map_multi {
        let declarations = crate::array_map_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_array_map_gradual {
        let declarations = crate::array_map_prelude::gradual_declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_array_keys_search {
        let declarations = crate::array_keys_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_preg_array {
        let declarations = crate::preg_array_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_array_walk {
        let declarations = crate::array_walk_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_is_callable_ext {
        let declarations = crate::is_callable_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_closure_from_callable {
        let declarations = crate::closure_from_callable_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_locale {
        let declarations = crate::locale_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_stream_args {
        let declarations = crate::stream_args_prelude::declarations();
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if needs_globals_snapshot {
        let declarations = crate::globals_snapshot_prelude::declarations(&program);
        inventory.record_program(BACKEND_GAP_GROUP, &declarations);
        combined.extend(declarations);
    }
    if inject_randomizer {
        combined.extend(crate::randomizer_prelude::declarations());
    }
    // Every helper here is named ONLY by the EIR lowering (a Rust constant), never by any PHP
    // source. `prune_unreachable_declarations` walks the PHP, finds no reference, and erases the
    // BODY while the call survives — the failure then surfaces a pass later as
    // `Call to undefined function __elephc_asort_mixed()`. Recording the group is the proof
    // that the injection already happened, which is what keeps the declarations alive.
    for source in sources {
        let tokens = crate::lexer::tokenize(source).expect("backend gap prelude must tokenize");
        let parsed = crate::parser::parse(&tokens).expect("backend gap prelude must parse");
        inventory.record_program(BACKEND_GAP_GROUP, &parsed);
        combined.extend(parsed);
    }
    // These helpers mirror PHP builtins that take arrays (`str_replace` with a count, `implode`
    // over mixed values, `file_put_contents` over an array, ...), so their `foreach` loops never
    // reach a user iterator. Declaring that lets reachability drop the by-name iterator-protocol
    // roots those loops would otherwise add, which kept every live `getIterator` in the program.
    // Done here, not in the pipeline, so every caller of this injector gets it.
    inventory.record_array_iteration_only(BACKEND_GAP_GROUP);
    combined.extend(program);
    combined
}

/// Compares canonical PHP class names case-insensitively and without a leading separator.
fn php_class_name_is(candidate: &str, expected: &str) -> bool {
    crate::names::php_symbol_key(candidate.trim_start_matches('\\'))
        == crate::names::php_symbol_key(expected.trim_start_matches('\\'))
}
