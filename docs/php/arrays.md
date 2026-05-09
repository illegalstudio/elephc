---
title: "Arrays"
description: "Indexed arrays, associative arrays, copy-on-write, and built-in array functions."
sidebar:
  order: 6
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

## Associative arrays
```php
<?php
$map = ["name" => "Alice", "city" => "Paris"];
echo $map["name"];       // Alice
$map["age"] = "30";      // add new key
```

Associative arrays use a hash table runtime. If later values do not match the first value type, the checker widens to internal `mixed` runtime shape.

Keys follow PHP's array-key normalization for integer and string keys. Integer keys remain integers, numeric strings such as `"1"` normalize to the integer key `1`, and strings with leading zeroes such as `"01"` remain string keys. This applies to literals, reads and writes, `foreach`, `array_keys()`, `array_search()`, `array_key_exists()`, `array_flip()`, JSON object keys, and associative array union.

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
| `count()` | `count($arr): int` | Number of elements |
| `array_push()` | `array_push($arr, $val): void` | Add element to end |
| `array_pop()` | `array_pop($arr): mixed` | Remove and return last element |
| `in_array()` | `in_array($needle, $arr): int` | Search for value (0/1) |
| `array_keys()` | `array_keys($arr): array` | Returns the array keys |
| `array_values()` | `array_values($arr): array` | Returns copy of values |
| `array_key_exists()` | `array_key_exists($key, $arr): bool` | Check if key exists |
| `array_search()` | `array_search($needle, $arr): int\|string\|false` | Search for value, returning an integer index for indexed arrays, the first matching associative-array key, or `false` if not found |
| `array_slice()` | `array_slice($arr, $offset [, $length]): array` | Extract a slice |
| `array_splice()` | `array_splice($arr, $offset [, $length]): array` | Remove/replace part |
| `array_chunk()` | `array_chunk($arr, $size): array` | Split into chunks |
| `array_merge()` | `array_merge($arr1, $arr2): array` | Merge two arrays |
| `array_combine()` | `array_combine($keys, $values): array` | Create array from keys/values |
| `array_fill()` | `array_fill($start, $num, $value): array` | Fill with values |
| `array_fill_keys()` | `array_fill_keys($keys, $value): array` | Fill with values using keys |
| `array_pad()` | `array_pad($arr, $size, $value): array` | Pad to length |
| `range()` | `range($start, $end): array` | Sequential integers |
| `array_diff()` | `array_diff($arr1, $arr2): array` | Values in $arr1 not in $arr2 |
| `array_intersect()` | `array_intersect($arr1, $arr2): array` | Values in both |
| `array_diff_key()` | `array_diff_key($arr1, $arr2): array` | Keys in $arr1 not in $arr2 |
| `array_intersect_key()` | `array_intersect_key($arr1, $arr2): array` | Keys in both |
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
| `array_filter()` | `array_filter($arr, $callback): array` | Filter where callback is truthy |
| `array_reduce()` | `array_reduce($arr, $callback, $init): int` | Reduce to single value |
| `array_walk()` | `array_walk($arr, $callback): void` | Call callback on each element |
| `usort()` | `usort($arr, $callback): void` | Sort with user comparison |
| `uksort()` | `uksort($arr, $callback): void` | Sort by key with user comparison |
| `uasort()` | `uasort($arr, $callback): void` | Sort with user comparison, maintain keys |
| `call_user_func()` | `call_user_func($callback, ...): mixed` | Call a callback value |
| `call_user_func_array()` | `call_user_func_array($callback, $args): mixed` | Call with args from array |
| `function_exists()` | `function_exists("name"): bool` | Check if function is defined |
| `isset()` | `isset($var): int` | Check if variable is defined |

> Callback arguments can be string literals, first-class callable values, anonymous functions, arrow functions, or variables holding captured closures.

**Not supported by design:** `compact()`, `extract()` require runtime variable-name tables and are listed in the roadmap's "Will not implement" section.

## Limitations
- Indexed arrays are homogeneous (except object elements may widen to shared parent class)
- Array union is supported for indexed+indexed and associative+associative operands; mixed indexed/associative union and heterogeneous indexed-array union are tracked in `ROADMAP.md`
