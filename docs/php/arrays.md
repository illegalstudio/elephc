---
title: "Arrays"
description: "Indexed arrays, associative arrays, copy-on-write, and built-in array functions."
sidebar:
  order: 8
---

## Indexed arrays
```php
<?php
$arr = [10, 20, 30];
echo $arr[0];          // 10
echo count($arr);      // 3
$arr[1] = 99;          // modify
$arr[] = 40;           // push
```

## String arrays
```php
<?php
$names = ["Alice", "Bob", "Charlie"];
foreach ($names as $name) {
    echo "Hello, " . $name . "\n";
}
```

## Heterogeneous indexed arrays
Indexed arrays can contain different value types. When element types differ, elephc stores the payloads as boxed `mixed` values internally.

```php
<?php
$items = [1, "two", true];
$items[] = 3.5;

echo $items[0]; // 1
echo $items[1]; // two
```

## Associative arrays
```php
<?php
$map = ["name" => "Alice", "city" => "Paris"];
echo $map["name"];       // Alice
$map["age"] = "30";      // add new key
```

Associative arrays use a hash table runtime. If later values do not match the first value type, the checker widens to internal `mixed` runtime shape.

Keys follow PHP's array-key normalization. Integer keys remain integers, booleans and floats normalize to integer keys, `null` normalizes to the empty-string key, numeric strings such as `"1"` normalize to the integer key `1`, and strings with leading zeroes such as `"01"` remain string keys. This applies to literals, reads and writes, `foreach`, `array_keys()`, `array_search()`, `array_key_exists()`, `array_flip()`, JSON object keys, and array union.

```php
<?php
$map = [1 => "one", "2" => "two", "02" => "leading"];

echo $map["1"];  // one
echo $map[2];    // two
echo $map["02"]; // leading
```

## Removing elements with unset

`unset($map[$key])` removes a single entry from an associative array. The removed key's owned
key and value storage is released, the live `count()` drops by one, and `isset()`, `foreach`,
and re-insertion all observe the entry as gone. Iteration order follows PHP: surviving entries
keep their original order, and re-adding a removed key appends it at the end.

```php
<?php
$map = ["a" => 1, "b" => 2, "c" => 3];
unset($map["b"]);

echo count($map);          // 2
echo isset($map["b"]) ? "y" : "n"; // n
$map["b"] = 9;             // re-added at the end
foreach ($map as $k => $v) { echo "$k=$v "; } // a=1 c=3 b=9
```

`unset()` also works on indexed arrays. PHP removes the key **without renumbering** the survivors,
so the array becomes sparse (a hole is left). The remaining keys keep their original values, and a
later `$arr[] = ...` append continues at `max_key + 1`.

```php
<?php
$arr = [1, 2, 3];
unset($arr[1]);
foreach ($arr as $k => $v) { echo "$k=$v "; } // 0=1 2=3 (no key 1)
$arr[] = 9;                                    // appended at key 3
echo isset($arr[1]) ? "y" : "n";               // n
```

`unset()` respects copy-on-write: removing a key from one array never mutates another array that
was assigned from it. Unsetting a key that is not present is a no-op.

```php
<?php
$a = ["x" => 1, "y" => 2];
$b = $a;
unset($b["x"]);
echo count($a); // 2 — original is untouched
echo count($b); // 1
```

> Removing an element from an array passed **by reference** (`function f(array &$a)`) is not yet
> supported and reports a compile error.

## Array union

`+` between arrays follows PHP union semantics: keys from the left operand win, and only keys that are missing from the left are copied from the right.

```php
<?php
$left = ["a" => "left", "b" => "keep"];
$right = ["a" => "right", "c" => "new"];
$result = $left + $right;

echo $result["a"]; // left
echo $result["c"]; // new
```

For indexed arrays, numeric keys are preserved. In elephc's dense indexed-array representation, this means the left side keeps indexes `0..count($left)-1`, and only the right suffix with higher numeric indexes is appended.

```php
<?php
$result = [10, 20] + [99, 88, 77];
echo $result[0]; // 10
echo $result[1]; // 20
echo $result[2]; // 77
```

Union also works across indexed and associative representations. Indexed positions become integer keys in the shared PHP key space, so an associative key `"0"` blocks right index `0`, while `"01"` remains a distinct string key.

```php
<?php
$left = ["0" => "left zero", "01" => "leading"];
$right = ["right zero", "right one"];
$result = $left + $right;

echo $result[0];    // left zero
echo $result[1];    // right one
echo $result["01"]; // leading
```

## Copy-on-write semantics
Arrays are shared until modified, matching PHP's by-value behavior:
```php
<?php
$a = [1, 2];
$b = $a;      // shares storage
$b[0] = 9;    // first write detaches $b
echo $a[0];   // 1
echo $b[0];   // 9
```

The same applies to function parameters and mutating built-ins (`array_push()`, `sort()`, `shuffle()`, etc.).

### Reference elements in array literals are not supported

PHP lets an array literal hold a *reference* to a variable, so writing through the
array writes through to that variable:

```php
<?php
$first = 1;
$r = [&$first];   // not supported by elephc
$r[0] = 9;
echo $first;      // PHP: 9
```

elephc rejects this at compile time:

```text
error[3:12]: Reference elements in array literals (`[&$x]`) are not supported:
an array element cannot alias a variable's storage.
```

The same applies to keyed elements (`['k' => &$a]`) and the legacy
`array(&$a)` spelling. elephc's arrays store plain values, and its only
reference form points *into* array storage (`$b =& $a[0]`), never out of it —
an element aliasing a local variable would be a pointer to a stack slot the
array can outlive. Assign the value and copy back afterwards, or alias an
existing element with `$b =& $a[0]`.

## Multi-dimensional arrays
```php
<?php
$matrix = [[1, 2], [3, 4]];
echo $matrix[0][1];    // 2
```

## Spread in array literals

The `...` spread operator flattens an array's elements into a new array literal, matching PHP semantics.

```php
<?php
$a = [1, 2, 3];
$b = [0, ...$a, 4];   // [0, 1, 2, 3, 4]
```

Spreading an associative array preserves string keys and reindexes integer-keyed entries to fresh sequential keys (continuing from the current largest integer key). Later spread operands overwrite earlier ones on string-key collision.

```php
<?php
$defaults = ['host' => '0.0.0.0', 'timeout' => 30];
$config = ['host' => 'localhost', 'port' => 8080];
$merged = [...$defaults, ...$config];
// ['host' => 'localhost', 'timeout' => 30, 'port' => 8080]
```

## Array destructuring

Array destructuring assigns array elements to writable targets. Both short syntax and `list(...)` are supported.

```php
<?php
[$first, , $third] = [10, 20, 30];
echo $first; // 10
echo $third; // 30

list($left, $right) = [1, 2];
```

Patterns can be nested, keyed, and can write to the same target forms as ordinary assignments.

```php
<?php
[[$a, $b], [$c, $d]] = [[1, 2], [3, 4]];

["name" => $name, "role" => $role] = ["role" => "admin", "name" => "Ada"];

$items = [0];
[$items[0], $items[]] = [5, 6];
```

PHP does not allow keyed and unkeyed entries in the same destructuring pattern, and elephc reports that as a compile-time error.

The same patterns can be used as a `foreach` value target — see
[foreach](./control-structures.md):

```php
<?php
foreach ([[1, 2], [3, 4]] as [$x, $y]) {
    echo $x + $y;
}
```

## Built-in array functions

| Function | Signature | Description |
|---|---|---|
| `count()` | `count($value [, $mode]): int` | Number of elements; on objects implementing `Countable`, dispatches to `count()`. A value that is not countable — `false`, an `int`, `float`, `string`, `null`, or resource arriving through a `mixed` — raises PHP 8's `TypeError` (`count(): Argument #1 ($value) must be of type Countable\|array, X given`) instead of answering `0`. `$mode` accepts `COUNT_NORMAL` (default) and `COUNT_RECURSIVE`; anything else throws `\ValueError`. `COUNT_RECURSIVE` is currently supported only where the receiver cannot hold a nested array — a nested receiver is a compile error rather than a wrong count |
| `array_count_values()` | `array_count_values($array): array` | Maps each distinct `int`/`string` value to its number of occurrences; other values are skipped with a warning |
| `array_push()` | `array_push($arr, $val): void` | Add element to end |
| `array_pop()` | `array_pop($arr): mixed` | Remove and return last element |
| `in_array()` | `in_array(mixed $needle, array $haystack, bool $strict = false): bool` | Search for a value. Omitted or `false` strictness uses PHP loose comparison for supported scalar/string values; `true` requires type-identical membership. |
| `array_keys()` | `array_keys($arr): array` | Returns the array keys |
| `array_values()` | `array_values($arr): array` | Returns copy of values |
| `array_key_exists()` | `array_key_exists($key, $arr): bool` | Check if key exists |
| `array_search()` | `array_search($needle, $haystack, $strict = false): int\|string\|false` | Search for value, returning an integer index for indexed arrays, the first matching associative-array key, or `false` if not found. `$strict` compares with `===` |
| `array_slice()` | `array_slice($arr, $offset [, $length [, $preserve_keys]]): array` | Extract a slice. `$preserve_keys` keeps the source integer keys and must be a literal `true`/`false`. |
| `array_splice()` | `array_splice($arr, $offset [, $length [, $replacement]]): array` | Remove a slice in place and return the removed elements. `$replacement` is inserted where the removed slice was: an array contributes its values, a bare scalar is treated as a one-element array, and `null` or `[]` inserts nothing. A replacement whose element type differs from the receiver's promotes the receiver to a heterogeneous array exactly as PHP does (`$a = [1,2,3]; array_splice($a, 1, 1, ["x"]);` leaves `[1, "x", 3]`). That promotion needs a receiver whose storage this call can retype, so it does not apply when the receiver is a by-reference parameter, a `&$x` binding, or an object/static property — those keep a named compile error instead of a mistyped insertion. |
| `array_chunk()` | `array_chunk($arr, $size [, $preserve_keys]): array` | Split into chunks. A `$size` of `0` or less throws `\ValueError`. `$preserve_keys` keeps each chunk's source integer keys and must be a literal `true`/`false`. |
| `array_merge()` | `array_merge($arr1, $arr2): array` | Merge two arrays |
| `array_merge_recursive()` | `array_merge_recursive($arr1, $arr2): array` | Recursively merge two arrays: integer keys append (renumbered), string keys that collide recurse when both values are arrays and otherwise combine into a list. Accepts associative arrays or **indexed arrays of scalars** (int/float/bool); nested indexed-array values are treated as opaque. |
| `array_replace()` | `array_replace($arr, $replacements): array` | Overwrite matching keys in `$arr` (in place, keeping position) and append new keys from `$replacements`; later values win. Accepts associative arrays or **indexed arrays of scalars** (int/float/bool). |
| `array_replace_recursive()` | `array_replace_recursive($arr, $replacements): array` | Like `array_replace()`, but when both values at a key are associative arrays they are merged recursively instead of overwritten. Accepts associative arrays or **indexed arrays of scalars** (int/float/bool); nested indexed arrays are overwritten, not merged. |
| `array_combine()` | `array_combine($keys, $values): array` | Create array from keys/values |
| `array_fill()` | `array_fill($start, $num, $value): array` | Fill with values. A negative `$num` throws `\ValueError`, and so does a `$num` above `2147483647` (`array_fill(): Argument #2 ($count) is too large`). |
| `array_fill_keys()` | `array_fill_keys($keys, $value): array` | Fill with values using keys |
| `array_pad()` | `array_pad($arr, $size, $value): array` | Pad to length; a negative `$size` pads on the left. A `$size` whose magnitude exceeds `1073741824` — including `PHP_INT_MIN`, whose magnitude is not representable — throws `\ValueError`. |
| `range()` | `range($start, $end, $step = 1): array` | Sequential integers. `$step`'s sign never picks the direction (`$start` vs `$end` does); a zero step, a negative step on an increasing range, or a step wider than the spanned interval raises `ValueError`, and so does a range of more than `1073741823` elements (`The supplied range exceeds the maximum array size: start=… end=… step=…`, naming the ordered endpoints and `abs($step)`) |
| `array_diff()` | `array_diff($arr1, $arr2): array` | Values in $arr1 not in $arr2 |
| `array_intersect()` | `array_intersect($arr1, $arr2): array` | Values in both |
| `array_diff_key()` | `array_diff_key($arr1, $arr2): array` | Keys in $arr1 not in $arr2 |
| `array_intersect_key()` | `array_intersect_key($arr1, $arr2): array` | Keys in both |
| `array_diff_assoc()` | `array_diff_assoc($arr1, $arr2): array` | Entries of $arr1 whose `(key, value)` pair is absent from $arr2 (values compared as `(string)$a === (string)$b`). Accepts associative arrays or **indexed arrays of scalars** (int/float/bool). |
| `array_intersect_assoc()` | `array_intersect_assoc($arr1, $arr2): array` | Entries of $arr1 whose `(key, value)` pair is present in $arr2 (values compared as strings). Accepts associative arrays or **indexed arrays of scalars** (int/float/bool). |
| `array_udiff()` | `array_udiff($arr1, $arr2, $cmp): array` | Two-array form: keeps values absent from $arr2, preserving $arr1 keys and value types. The comparator result is cast to int before testing equality with zero. Supports closures, function names and first-class callables. |
| `array_uintersect()` | `array_uintersect($arr1, $arr2, $cmp): array` | Two-array form: keeps values also present in $arr2, preserving $arr1 keys and value types. The comparator result is cast to int before testing equality with zero. |
| `array_unique()` | `array_unique($arr): array` | Remove duplicates, keeping each survivor's original key — an indexed input therefore returns a sparse, hash-shaped result (keys `0, 1, 3` for `[1,2,2,3,1]`), exactly as in PHP |
| `array_reverse()` | `array_reverse($arr, $preserve_keys = false): array` | Reverse order. `$preserve_keys` must be a literal `bool` in AOT mode because it changes the result shape: `true` keeps the original integer keys, producing an integer-keyed array |
| `array_flip()` | `array_flip($arr): array` | Exchange keys and values, normalizing integer and numeric-string result keys |
| `array_shift()` | `array_shift($arr): mixed` | Remove and return first |
| `array_unshift()` | `array_unshift($arr, ...$values): int` | Prepend one or more elements, in source order, and return the new count |
| `array_sum()` | `array_sum($arr): int\|float` | Sum of values |
| `array_product()` | `array_product($arr): int\|float` | Product of values |
| `array_column()` | `array_column($arr, $column_key): array` | Extract column from array of assoc rows |
| `array_is_list()` | `array_is_list($arr): bool` | `true` if the keys are exactly `0..count-1` in order (the empty array is a list) |
| `array_key_first()` | `array_key_first($arr): int\|string\|null` | First key in insertion order, or `null` if the array is empty |
| `array_key_last()` | `array_key_last($arr): int\|string\|null` | Last key in insertion order, or `null` if the array is empty |
| `key()` | `key($arr): int\|string\|null` | Key under the array's internal pointer, or `null` once the pointer is off either end |
| `current()` | `current($arr): mixed` | Element under the array's internal pointer, or `false` once the pointer is off either end |
| `next()` | `next(&$arr): mixed` | Advance the internal pointer one position and return the new element, or `false` |
| `prev()` | `prev(&$arr): mixed` | Rewind the internal pointer one position and return the new element, or `false` |
| `reset()` | `reset(&$arr): mixed` | Move the internal pointer to the first element and return it, or `false` for an empty array |
| `end()` | `end(&$arr): mixed` | Move the internal pointer to the last element and return it, or `false` for an empty array |
| `sort()` | `sort($arr): void` | Sort ascending (in-place) |
| `rsort()` | `rsort($arr): void` | Sort descending |
| `asort()` | `asort($arr): void` | Sort by value, maintain keys |
| `arsort()` | `arsort($arr): void` | Sort by value desc, maintain keys |
| `ksort()` | `ksort($arr): bool` | Sort by key ascending with `SORT_REGULAR`. On an indexed array this is a no-op, because its keys are already the ascending slot positions `0..n-1` |
| `krsort()` | `krsort($arr): bool` | Sort by key descending with `SORT_REGULAR`. A non-empty indexed array is promoted to associative storage so its numeric keys can appear in descending iteration order |
| `natsort()` | `natsort($arr): void` | Natural order sort |
| `natcasesort()` | `natcasesort($arr): void` | Case-insensitive natural sort |
| `shuffle()` | `shuffle($arr): void` | Randomly shuffle (in-place) |
| `array_multisort()` | `array_multisort($arr1, $arr2): bool` | Sort equal-length indexed arrays in place by ascending row order: compare `$arr1`, then `$arr2` for ties. AOT accepts two concrete integer arrays or two boxed scalar arrays (for example through declared `array` parameters), not one of each. Concrete string/float arrays, associative arrays, sort flags, and more than two arrays are unsupported. |
| `array_rand()` | `array_rand($arr): int` | Pick one random key |
| `array_map()` | `array_map($callback, $arr): array` | Apply callback to each element |
| `array_filter()` | `array_filter($arr, $callback = null, $mode = 0): array` | Preserve keys and values accepted by the callback; omitted/null callbacks remove empty values |
| `array_reduce()` | `array_reduce($arr, $callback, $init): int` | Reduce to single value |
| `array_walk()` | `array_walk($arr, $callback): void` | Call a callback on each element. Boxed PHP arrays support writable value references and keys, subject to the reference-escape restrictions below. |
| `array_walk_recursive()` | `array_walk_recursive($arr, $callback): void` | Apply a callback to non-array leaves. The concrete-array path requires homogeneous scalar leaves and passes values without keys. The boxed PHP-array path supports writable leaf references and keys, subject to the restrictions below. |
| `array_find()` | `array_find($arr, $callback): mixed` | (PHP 8.4) Returns the first element for which `$callback($value)` is truthy, or `null` if none match. |
| `array_any()` | `array_any($arr, $callback): bool` | (PHP 8.4) `true` if `$callback($value)` is truthy for at least one element. |
| `array_all()` | `array_all($arr, $callback): bool` | (PHP 8.4) `true` if `$callback($value)` is truthy for every element. |
| `usort()` | `usort($arr, $callback): void` | Sort with user comparison |
| `uksort()` | `uksort($arr, $callback): void` | Sort by key with user comparison |
| `uasort()` | `uasort($arr, $callback): void` | Sort with user comparison, maintain keys |
| `call_user_func()` | `call_user_func($callback, ...): mixed` | Call a callback value |
| `call_user_func_array()` | `call_user_func_array($callback, $args): mixed` | Call with args from array |
| `function_exists()` | `function_exists(string $name): bool` | Check if a global or fully-qualified function name is defined. A literal name const-folds; any other string expression is matched case-insensitively at run time against the functions this binary declares |
| `isset()` | `isset($var, ...$vars): int` | Check that every variable or offset is defined and not null. Like PHP, the probed variable does not have to exist: `isset($neverDefined)` is `false`, `empty($neverDefined)` is `true`, `$neverDefined ?? "d"` is `"d"`, and `unset($neverDefined)` is a no-op. A name that is *also* assigned elsewhere in the same scope must still be defined before it is probed |

`array_filter()` preserves integer and string keys, including holes, and supports declared PHP arrays with heterogeneous values. Its callback can receive values (`0`), both value and key (`ARRAY_FILTER_USE_BOTH`, `1`), or keys (`ARRAY_FILTER_USE_KEY`, `2`). PHP profiles through 8.5 treat other integer modes as value-only; PHP 8.6 rejects them with a catchable `ValueError`, including for empty arrays and null callbacks. The `ARRAY_FILTER_USE_VALUE` name is available in PHP 8.6. Callback results use PHP truthiness rather than requiring a boolean return.

`array_map()` also resolves boxed callbacks extracted from PHP arrays, including
captured closures, function names, packed `[object, method]` pairs, and invokable
objects available in the compiled program. Invalid boxed callbacks throw
`TypeError`. With one source array, a `null` callback returns an independent
array value with the original keys and copy-on-write behavior. The AOT path
currently accepts one source array, not the multi-array zip form.

> Callback arguments can be string literals, runtime string names for user functions, first-class callable values, anonymous functions, arrow functions, or variables holding captured closures. `array_map()`, `array_filter()`, `array_reduce()`, `array_walk()`, `usort()`, `uksort()`, and `uasort()` resolve runtime string callback variables through descriptor dispatch. `array_map()` stores mixed result elements when the selected callback return shape is only known at runtime. `array_map()` also runs over a heterogeneous (boxed `mixed`) input array: each element is passed to the callback as a `mixed` value, so a callback with a `mixed` (or untyped) parameter sees and can return each element with its original runtime type.
> When a parameter is declared only as `array`, its element type is initially unknown. Array-callback checking preserves explicit callback parameter declarations and uses them to type the closure body instead of fabricating an `int` element. Known element types are still checked normally, and this contextual rule does not make `mixed` globally compatible with object, array, or other refcounted declarations. `array_map()` currently rejects known object-element arrays because its callback runtime does not yet support that input layout.
> `call_user_func_array()` also accepts dynamic indexed and associative argument arrays for callbacks with a known signature, including userland variadic callbacks. When a callable value has no single static signature at the call site, elephc emits an AOT runtime dispatch over user functions and closure/FCC wrappers available in that codegen context, then applies the matched target's descriptor metadata: parameter names, defaults, by-reference flags, variadic position, return shape, captures, hidden receiver arguments, and callable shape. Runtime string callback names dispatch over user functions, supported builtins, and public static-method strings by case-insensitive name matching, materialize the matched descriptor, and invoke its generated descriptor invoker. Descriptor invokers receive a temporary boxed Mixed clone of the argument container and inspect its runtime tag to handle indexed arrays and associative hashes through the same signature-level wrapper, so the source `$args` remains usable with its original static layout after the call. String keys bind named parameters; unconsumed string and numeric keys are copied into `...$rest` for variadic callbacks. Dynamic arrays passed to by-reference callback parameters use temporary reference cells, so callback writes do not mutate the source argument array.

Unannotated callback parameters are typed from the array in every array builtin that takes a callback — `array_all()`, `array_any()`, `array_filter()`, `array_find()`, `array_map()`, `array_reduce()`, `array_udiff()`, `array_uintersect()`, `array_walk()`, `array_walk_recursive()`, `uasort()`, `uksort()` and `usort()`. Value parameters get the element type and key parameters get the key type, so `array_filter($words, fn($v) => strlen($v) > 3)`, `uksort($byName, fn($a, $b) => strlen($a) <=> strlen($b))` and `array_walk($byName, function ($v, $k) { echo strlen($k); })` all check without hand-written type hints. Explicit hints stay authoritative.

### Walking boxed PHP arrays

The AOT path for boxed PHP arrays, including declared `array` parameters, passes
the callback a writable element reference and its key. Recursive walks do this
for leaf values. Copies made before the walk keep their original values.

This path requires a visible native function, closure, or method descriptor.
Opaque eval/extern callbacks, first-class wrapper descriptors, and builtin adapters are
rejected. The borrowed element reference must not escape the callback through a
closure capture or a property reference: those operations throw a catchable
`Error`. Ordinary compiler-managed references are unaffected. These restrictions
are specific to the boxed walk path and are not a claim of full PHP parity.

### Sorting runtime-typed indexed arrays

In compiled (AOT) code, [`sort()`](./builtins/array/sort.md) and
[`rsort()`](./builtins/array/rsort.md) accept indexed arrays whose elements use
boxed `mixed` storage (`Array(Mixed)` internally). Scalars (`bool`, `int`,
`float`, and `string`) and `null` are supported and use the runtime's PHP scalar
comparison helper.

Before sorting, elephc checks every element. If any element is non-scalar, such
as a nested array, object, resource, or boxed callable, execution terminates
with:

```text
Fatal error: sorting Mixed arrays containing non-scalar values is not supported
```

This is a deliberate AOT restriction: full PHP container ordering is not yet
implemented, so these values produce an explicit fatal error instead of a
silently incorrect order.

### Sorting an associative array

`ksort()`, `krsort()`, `asort()` and `arsort()` reorder an associative array by rewriting its
iteration order only — every key stays attached to its own value, later key lookups and inserts
keep working, and PHP's copy-on-write still applies, so a copy taken before the call keeps the
original order. All four return `true` after sorting:

```php
$byName = ["b" => 2, "a" => 3, "c" => 1];
$snapshot = $byName;
ksort($byName);
echo implode(",", array_keys($byName));   // a,b,c
echo implode(",", array_keys($snapshot)); // b,a,c
```

`ksort()` and `krsort()` currently accept exactly one argument and always use PHP's default
`SORT_REGULAR` comparison. PHP's optional `$flags` argument (`SORT_NUMERIC`, `SORT_STRING`,
`SORT_NATURAL`, `SORT_FLAG_CASE`, and related modes) is not implemented yet; passing it is a
compile-time arity error. Full flag parity is tracked in
[issue #699](https://github.com/illegalstudio/elephc/issues/699).

For indexed storage, ascending `ksort()` leaves keys `0..n-1` in place. Descending `krsort()`
promotes a non-empty indexed array to associative storage, preserving each numeric key/value pair:

```php
$values = ["zero", "one", "two"];
krsort($values);
echo implode(",", array_keys($values)); // 2,1,0
echo $values[0];                        // zero
```

Keys are ordered with PHP's standard comparison, not byte-wise, so numeric keys compare as
numbers even against string keys: `[10 => …, "9" => …, "Banana" => …]` sorts as `9`, `10`,
`'Banana'`. Ties keep their original relative order in every direction, matching PHP 8's stable
sorts — `["b" => 2, "d" => 2, "a" => 3]` keeps `b` before `d` under both `asort()` and `arsort()`.

One known deviation: when an array mixes integer keys with string keys that are *not* numeric,
PHP's own key comparison is not transitive (for `10`, `"20a"` and `6`, PHP reports
`"20a" < 10`, `10 < 6`… and `6 < "20a"` is false), so no ordering satisfies every pair. In that
case elephc's result and PHP's result are both consistent with the comparison but can differ,
because each resolves the cycle through its own sort algorithm.

`uasort()`, `uksort()`, `natsort()` and `natcasesort()` do not yet accept an associative array and
report a clear unsupported-feature error.

`usort()` and `uasort()` sort arrays of **objects** as well as scalars. The comparator receives each element as its object handle, so an unannotated comparator's parameters are typed from the array element automatically — `usort($items, fn($a, $b) => $a->weight <=> $b->weight)` works without writing `($a, $b)` type hints, and `usort($dates, fn($a, $b) => $a <=> $b)` over `DateTime`/`DateTimeImmutable` compares by instant. Explicit hints (`function (Item $a, Item $b)`) are equally accepted. `usort()` also sorts arrays of **strings**: `usort($words, fn($a, $b) => strlen($a) <=> strlen($b))` reorders the string array in place, keeps elements the comparator reports equal in their original relative order, and renumbers the keys from zero like PHP. `uasort()` and `uksort()` over a string array still report a clear unsupported-feature error, because they must preserve the original key association.

Array builtins that take their first argument **by reference** — `sort()`, `rsort()`, `asort()`, `arsort()`, `ksort()`, `krsort()`, `natsort()`, `natcasesort()`, `shuffle()`, `usort()`, `uasort()`, `uksort()`, `array_push()`, `array_pop()`, `array_shift()`, `array_unshift()`, `array_splice()` and `array_walk()` — mutate the caller's storage whether that storage is a local variable, an object property (`sort($obj->items)`, `sort($this->items)`, `sort($outer->inner->items)`), a static property (`sort(Foo::$items)`, `sort(self::$items)`), or a container element (`sort($rows[0])`, `sort($map["k"])`). PHP's copy-on-write applies as usual: a copy taken before the call keeps the original element order.

```php
class Basket { public $items = [3, 1, 2]; }
$b = new Basket();
$snapshot = $b->items;
usort($b->items, fn($x, $y) => $x <=> $y);
echo implode(",", $b->items);   // 1,2,3
echo implode(",", $snapshot);   // 3,1,2 — the copy is untouched
```

A receiver elephc cannot resolve to writable storage — a nullsafe read (`sort($obj?->items)`, which PHP rejects too) or a property whose type is only known as `mixed` — is reported as a named unsupported-feature error rather than compiled into a silent no-op.

The same applies when the receiver is a **by-reference parameter**: `function f(array &$a) { array_unshift($a, 1); }` mutates the caller's array, and so do `array_pop()`, `array_shift()`, `array_splice()`, the sort family, `array_multisort()` and an associative insert such as `$a["k"] = 1`. All of them copy-on-write split their receiver first, and prepending or splicing in a replacement can additionally relocate its storage, so the new pointer is published through the parameter's reference cell.

`array_reduce()` folds arrays of **strings** too — `array_reduce($words, fn($carry, $word) => $carry + strlen($word), 0)` passes each element to the callback as a string. The accumulator itself must still be an `int` or `bool`; a string accumulator reports a clear unsupported-feature error.

## The internal array pointer

`key()`, `current()`, `next()`, `prev()`, `reset()` and `end()` operate on PHP's internal
array pointer:

```php
$stock = ["apples" => 3, "pears" => 7, "plums" => 0];

reset($stock);
while (($qty = current($stock)) !== false) {
    echo key($stock), "=", $qty, " ";   // apples=3 pears=7 plums=0
    next($stock);
}

echo end($stock);                        // 0
echo key($stock);                        // plums
```

Semantics match PHP exactly for the supported receiver shape:

- A freshly built array starts with its pointer on the first element, so `current()` and
  `key()` work without calling `reset()` first.
- There is a single invalid position, and it is one-way. Running off the back with
  `next()` or off the front with `prev()` leaves the pointer invalid; the opposite
  direction does **not** walk back in. Only `reset()` and `end()` restore a valid pointer.
- While invalid, `current()`/`next()`/`prev()`/`reset()`/`end()` return `false` and `key()`
  returns `null`. An empty array is always in that state.
- `foreach` never moves the pointer, by value or by reference — PHP 7+ iterates an
  internal copy, and elephc's `foreach` keeps its cursor in the stack frame.
- Binding the variable to a different array rewinds its pointer to the first element,
  because in PHP the pointer belongs to the hashtable that was replaced.
- Associative arrays are walked in insertion order and `key()` reports the real key.

### Receiver must be a plain variable

PHP stores the pointer inside the array's hashtable. elephc's array and hash headers have
no room for it — widening either would shift every offset in every runtime helper — so the
pointer lives in a hidden cursor slot the compiler allocates **beside the array local**.

The direct consequence is that the argument must be a plain variable. A property, an array
element, a call result, or any other expression has nowhere to keep a cursor, so elephc
reports a compile error rather than silently operating on a detached one:

```php
echo key($obj->rows);      // compile error: key() argument must be an array variable
echo current(rows());      // compile error: current() argument must be an array variable
next($grid[0]);            // compile error: next() argument must be an array variable
```

Copy the value into a local first (`$rows = $obj->rows;`) and walk that.

### Known incompatibilities

Because the cursor is attached to the variable instead of to the array value, three PHP
behaviours differ. All three involve a pointer that has been moved away from the first
element and then observed through a *different* variable.

| Situation | PHP | elephc |
|---|---|---|
| `$a = [1,2,3]; next($a); $b = $a; echo key($b);` | `1` — the copy inherits the pointer at copy time | `0` — `$b` starts its own cursor |
| `function f($x) { return key($x); } $a = [1,2,3]; next($a); echo f($a);` | `1` — the by-value parameter inherits the caller's pointer | `0` — the parameter's cursor starts fresh |
| `$a = [3,1,2]; next($a); sort($a); echo key($a);` | `0` — `sort()`/`array_shift()`/`array_splice()` rewind the pointer | `1` — those builtins leave the cursor untouched |

Everything else — including `$a[] = x` and `$a[$k] = v` **keeping** the pointer where it
is, which PHP also does — behaves the same.

One performance note: reading through the cursor is `O(1)` for indexed arrays but walks
the insertion-order chain for associative arrays, so `current()`/`key()` on a hash cost
`O(position)`. A full `while (current(...)) { next(...); }` traversal of a large hash is
therefore quadratic; prefer `foreach` when you do not need the pointer.

**Not supported yet:** `compact()` and `extract()` need dynamic access to the
current variable scope. Magician's materialized named scope makes that behavior
feasible, but these functions are not wired into the compiler or interpreter
today. Use an associative array explicitly in portable elephc code.
