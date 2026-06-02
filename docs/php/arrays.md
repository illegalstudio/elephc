---
title: "Arrays"
description: "Indexed arrays, associative arrays, copy-on-write, and built-in array functions."
sidebar:
  order: 7
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

Keys follow PHP's array-key normalization for integer and string keys. Integer keys remain integers, numeric strings such as `"1"` normalize to the integer key `1`, and strings with leading zeroes such as `"01"` remain string keys. This applies to literals, reads and writes, `foreach`, `array_keys()`, `array_search()`, `array_key_exists()`, `array_flip()`, JSON object keys, and array union.

```php
<?php
$map = [1 => "one", "2" => "two", "02" => "leading"];

echo $map["1"];  // one
echo $map[2];    // two
echo $map["02"]; // leading
```

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

## Multi-dimensional arrays
```php
<?php
$matrix = [[1, 2], [3, 4]];
echo $matrix[0][1];    // 2
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

## Built-in array functions

| Function | Signature | Description |
|---|---|---|
| `count()` | `count($arr_or_countable): int` | Number of elements; on objects implementing `Countable`, dispatches to `count()` |
| `array_push()` | `array_push($arr, $val): void` | Add element to end |
| `array_pop()` | `array_pop($arr): mixed` | Remove and return last element |
| `in_array()` | `in_array($needle, $arr): int` | Search for value (0/1) |
| `array_keys()` | `array_keys($arr): array` | Returns the array keys |
| `array_values()` | `array_values($arr): array` | Returns copy of values |
| `array_key_exists()` | `array_key_exists($key, $arr): bool` | Check if key exists |
| `array_key_first()` | `array_key_first($arr): int\|string\|null` | First key in insertion order, or `null` if the array is empty |
| `array_key_last()` | `array_key_last($arr): int\|string\|null` | Last key in insertion order, or `null` if the array is empty |
| `array_is_list()` | `array_is_list($arr): bool` | `true` if the keys are exactly `0..count-1` in order (the empty array is a list) |
| `array_search()` | `array_search($needle, $arr): int\|string\|false` | Search for value, returning an integer index for indexed arrays, the first matching associative-array key, or `false` if not found |
| `array_slice()` | `array_slice($arr, $offset [, $length]): array` | Extract a slice |
| `array_splice()` | `array_splice($arr, $offset [, $length]): array` | Remove a slice in place and return the removed elements |
| `array_chunk()` | `array_chunk($arr, $size): array` | Split into chunks |
| `array_merge()` | `array_merge($arr1, $arr2): array` | Merge two arrays |
| `array_merge_recursive()` | `array_merge_recursive($arr1, $arr2): array` | Recursively merge two arrays: integer keys append (renumbered), string keys that collide recurse when both values are arrays and otherwise combine into a list. Accepts associative arrays or **indexed arrays of scalars** (int/float/bool); nested indexed-array values are treated as opaque. |
| `array_replace()` | `array_replace($arr, $replacements): array` | Overwrite matching keys in `$arr` (in place, keeping position) and append new keys from `$replacements`; later values win. Accepts associative arrays or **indexed arrays of scalars** (int/float/bool). |
| `array_replace_recursive()` | `array_replace_recursive($arr, $replacements): array` | Like `array_replace()`, but when both values at a key are associative arrays they are merged recursively instead of overwritten. Accepts associative arrays or **indexed arrays of scalars** (int/float/bool); nested indexed arrays are overwritten, not merged. |
| `array_combine()` | `array_combine($keys, $values): array` | Create array from keys/values |
| `array_fill()` | `array_fill($start, $num, $value): array` | Fill with values |
| `array_fill_keys()` | `array_fill_keys($keys, $value): array` | Fill with values using keys |
| `array_pad()` | `array_pad($arr, $size, $value): array` | Pad to length |
| `range()` | `range($start, $end): array` | Sequential integers |
| `array_diff()` | `array_diff($arr1, $arr2): array` | Values in $arr1 not in $arr2 |
| `array_intersect()` | `array_intersect($arr1, $arr2): array` | Values in both |
| `array_diff_key()` | `array_diff_key($arr1, $arr2): array` | Keys in $arr1 not in $arr2 |
| `array_intersect_key()` | `array_intersect_key($arr1, $arr2): array` | Keys in both |
| `array_udiff()` | `array_udiff($arr1, $arr2, $cmp): array` | Values in $arr1 not in $arr2, equality decided by the two-argument comparator (`$cmp($a, $b) === 0`). Supports string / function / non-capturing closure comparators. |
| `array_uintersect()` | `array_uintersect($arr1, $arr2, $cmp): array` | Values in both arrays, equality decided by the comparator (`$cmp($a, $b) === 0`). |
| `array_diff_assoc()` | `array_diff_assoc($arr1, $arr2): array` | Entries of $arr1 whose `(key, value)` pair is absent from $arr2 (values compared as `(string)$a === (string)$b`). Accepts associative arrays or **indexed arrays of scalars** (int/float/bool). |
| `array_intersect_assoc()` | `array_intersect_assoc($arr1, $arr2): array` | Entries of $arr1 whose `(key, value)` pair is present in $arr2 (values compared as strings). Accepts associative arrays or **indexed arrays of scalars** (int/float/bool). |
| `array_unique()` | `array_unique($arr): array` | Remove duplicates |
| `array_reverse()` | `array_reverse($arr): array` | Reverse order |
| `array_flip()` | `array_flip($arr): array` | Exchange keys and values, normalizing integer and numeric-string result keys |
| `array_shift()` | `array_shift($arr): mixed` | Remove and return first |
| `array_unshift()` | `array_unshift($arr, $value): int` | Prepend element |
| `array_sum()` | `array_sum($arr): int\|float` | Sum of values |
| `array_product()` | `array_product($arr): int\|float` | Product of values |
| `array_column()` | `array_column($arr, $column_key): array` | Extract column from array of assoc rows |
| `sort()` | `sort($arr): void` | Sort ascending (in-place) |
| `rsort()` | `rsort($arr): void` | Sort descending |
| `asort()` | `asort($arr): void` | Sort by value, maintain keys |
| `arsort()` | `arsort($arr): void` | Sort by value desc, maintain keys |
| `ksort()` | `ksort($arr): void` | Sort by key ascending |
| `krsort()` | `krsort($arr): void` | Sort by key descending |
| `natsort()` | `natsort($arr): void` | Natural order sort |
| `natcasesort()` | `natcasesort($arr): void` | Case-insensitive natural sort |
| `shuffle()` | `shuffle($arr): void` | Randomly shuffle (in-place) |
| `array_rand()` | `array_rand($arr): int` | Pick one random key |
| `array_map()` | `array_map($callback, $arr): array` | Apply callback to each element |
| `array_walk_recursive()` | `array_walk_recursive($arr, $callback): void` | Apply `$callback` to each non-array leaf value, recursing into nested indexed/associative arrays. Leaf values must share a scalar type (consistent with `array_walk`: leaf passed by value, no key argument). |
| `array_multisort()` | `array_multisort($arr1, $arr2): bool` | Sort `$arr1` ascending (stable) and reorder `$arr2` in tandem; both are sorted in place (by reference). **Two indexed arrays of scalar elements**; sort flags, descending order, and >2 arrays are follow-ups. |
| `array_find()` | `array_find($arr, $callback): mixed` | (PHP 8.4) Returns the first element for which `$callback($value)` is truthy, or `null` if none match. |
| `array_any()` | `array_any($arr, $callback): bool` | (PHP 8.4) `true` if `$callback($value)` is truthy for at least one element. |
| `array_all()` | `array_all($arr, $callback): bool` | (PHP 8.4) `true` if `$callback($value)` is truthy for every element. |
| `array_filter()` | `array_filter($arr, $callback): array` | Filter where callback is truthy |
| `array_reduce()` | `array_reduce($arr, $callback, $init): int` | Reduce to single value |
| `array_walk()` | `array_walk($arr, $callback): void` | Call callback on each element |
| `usort()` | `usort($arr, $callback): void` | Sort with user comparison |
| `uksort()` | `uksort($arr, $callback): void` | Sort by key with user comparison |
| `uasort()` | `uasort($arr, $callback): void` | Sort with user comparison, maintain keys |
| `call_user_func()` | `call_user_func($callback, ...): mixed` | Call a callback value |
| `call_user_func_array()` | `call_user_func_array($callback, $args): mixed` | Call with args from array |
| `function_exists()` | `function_exists("name"): bool` | Check if function is defined |
| `isset()` | `isset($var, ...$vars): int` | Check that every variable or offset is defined and not null |

> Callback arguments can be string literals, runtime string names for user functions, first-class callable values, anonymous functions, arrow functions, or variables holding captured closures. `array_map()`, `array_filter()`, `array_reduce()`, `array_walk()`, `usort()`, `uksort()`, and `uasort()` resolve runtime string callback variables through descriptor dispatch. `array_map()` stores mixed result elements when the selected callback return shape is only known at runtime.
> `call_user_func_array()` also accepts dynamic indexed and associative argument arrays for callbacks with a known signature, including userland variadic callbacks. When a callable value has no single static signature at the call site, elephc emits an AOT runtime dispatch over user functions and closure/FCC wrappers available in that codegen context, then applies the matched target's descriptor metadata: parameter names, defaults, by-reference flags, variadic position, return shape, captures, hidden receiver arguments, and callable shape. Runtime string callback names dispatch over user functions, supported builtins, and public static-method strings by case-insensitive name matching, materialize the matched descriptor, and invoke its generated descriptor invoker. Descriptor invokers receive a temporary boxed Mixed clone of the argument container and inspect its runtime tag to handle indexed arrays and associative hashes through the same signature-level wrapper, so the source `$args` remains usable with its original static layout after the call. String keys bind named parameters; unconsumed string and numeric keys are copied into `...$rest` for variadic callbacks. Dynamic arrays passed to by-reference callback parameters use temporary reference cells, so callback writes do not mutate the source argument array.

**Not supported by design:** `compact()`, `extract()` require runtime variable-name tables and are listed in the roadmap's "Will not implement" section.
