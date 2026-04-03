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
| `array_search()` | `array_search($needle, $arr): int` | Search for value, return key. Returns `-1` if not found |
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
| `array_flip()` | `array_flip($arr): array` | Exchange keys and values |
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
| `array_map()` | `array_map("callback", $arr): array` | Apply callback to each element |
| `array_filter()` | `array_filter($arr, "callback"): array` | Filter where callback is truthy |
| `array_reduce()` | `array_reduce($arr, "callback", $init): int` | Reduce to single value |
| `array_walk()` | `array_walk($arr, "callback"): void` | Call callback on each element |
| `usort()` | `usort($arr, "callback"): void` | Sort with user comparison |
| `uksort()` | `uksort($arr, "callback"): void` | Sort by key with user comparison |
| `uasort()` | `uasort($arr, "callback"): void` | Sort with user comparison, maintain keys |
| `call_user_func()` | `call_user_func("name", ...): mixed` | Call function by name |
| `call_user_func_array()` | `call_user_func_array("name", $args): mixed` | Call with args from array |
| `function_exists()` | `function_exists("name"): bool` | Check if function is defined |
| `isset()` | `isset($var): int` | Check if variable is defined |

> Callback arguments can be string literals, anonymous functions, or arrow functions.

**Not yet supported:** `compact()`, `extract()`.

## Limitations
- No array union operator (`+`)
- Indexed arrays are homogeneous (except object elements may widen to shared parent class)
