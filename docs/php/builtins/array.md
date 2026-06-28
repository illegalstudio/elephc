---
title: "Array builtins"
description: "Builtins in the Array category."
sidebar:
  order: 102
---

## Array builtins

| Function | Signature | Returns |
|---|---|---|
| [`array_chunk()`](./array/array_chunk.md) | `(array $array, int $length, bool $preserve_keys): array` | `array` |
| [`array_column()`](./array/array_column.md) | `(array $array, string $column_key, string $index_key): array` | `array` |
| [`array_combine()`](./array/array_combine.md) | `(array $keys, array $values): array` | `array` |
| [`array_diff()`](./array/array_diff.md) | `(array $array, ...$arrays): array` | `array` |
| [`array_diff_key()`](./array/array_diff_key.md) | `(array $array, ...$arrays): array` | `array` |
| [`array_fill()`](./array/array_fill.md) | `(int $start_index, int $count, mixed $value): array` | `array` |
| [`array_fill_keys()`](./array/array_fill_keys.md) | `(array $keys, mixed $value): array` | `array` |
| [`array_filter()`](./array/array_filter.md) | `(array $array, callable $callback, int $mode): array` | `array` |
| [`array_flip()`](./array/array_flip.md) | `(array $array): float` | `float` |
| [`array_intersect()`](./array/array_intersect.md) | `(array $array, ...$arrays): array` | `array` |
| [`array_intersect_key()`](./array/array_intersect_key.md) | `(array $array, ...$arrays): array` | `array` |
| [`array_key_exists()`](./array/array_key_exists.md) | `(string $key, array $array): bool` | `bool` |
| [`array_keys()`](./array/array_keys.md) | `(array $array, string $filter_value, bool $strict): array` | `array` |
| [`array_map()`](./array/array_map.md) | `(callable $callback, array $array, ...$arrays): array` | `array` |
| [`array_merge()`](./array/array_merge.md) | `(...$arrays): array` | `array` |
| [`array_pad()`](./array/array_pad.md) | `(array $array, int $length, mixed $value): array` | `array` |
| [`array_pop()`](./array/array_pop.md) | `(array $array): mixed` | `mixed` |
| [`array_product()`](./array/array_product.md) | `(array $array): float` | `float` |
| [`array_push()`](./array/array_push.md) | `(array $array, ...$values): void` | `void` |
| [`array_rand()`](./array/array_rand.md) | `(array $array, int $num): int` | `int` |
| [`array_reduce()`](./array/array_reduce.md) | `(array $array, callable $callback, mixed $initial): int` | `int` |
| [`array_reverse()`](./array/array_reverse.md) | `(array $array, bool $preserve_keys): array` | `array` |
| [`array_search()`](./array/array_search.md) | `(mixed $needle, array $haystack, bool $strict): mixed` | `mixed` |
| [`array_shift()`](./array/array_shift.md) | `(array $array): mixed` | `mixed` |
| [`array_slice()`](./array/array_slice.md) | `(array $array, int $offset, int $length, bool $preserve_keys): array` | `array` |
| [`array_splice()`](./array/array_splice.md) | `(array $array, int $offset, int $length, array $replacement): array` | `array` |
| [`array_sum()`](./array/array_sum.md) | `(array $array): float` | `float` |
| [`array_unique()`](./array/array_unique.md) | `(array $array, int $flags): array` | `array` |
| [`array_unshift()`](./array/array_unshift.md) | `(array $array, ...$values): int` | `int` |
| [`array_values()`](./array/array_values.md) | `(array $array): array` | `array` |
| [`array_walk()`](./array/array_walk.md) | `(array $array, callable $callback, mixed $arg): void` | `void` |
| [`arsort()`](./array/arsort.md) | `(array $array, int $flags): bool` | `bool` |
| [`asort()`](./array/asort.md) | `(array $array, int $flags): bool` | `bool` |
| [`count()`](./array/count.md) | `(array $value, int $mode): int` | `int` |
| [`in_array()`](./array/in_array.md) | `(mixed $needle, array $haystack, bool $strict): mixed` | `mixed` |
| [`krsort()`](./array/krsort.md) | `(array $array, int $flags): bool` | `bool` |
| [`ksort()`](./array/ksort.md) | `(array $array, int $flags): bool` | `bool` |
| [`natcasesort()`](./array/natcasesort.md) | `(array $array): bool` | `bool` |
| [`natsort()`](./array/natsort.md) | `(array $array): bool` | `bool` |
| [`range()`](./array/range.md) | `(mixed $start, mixed $end, int $step): array` | `array` |
| [`rsort()`](./array/rsort.md) | `(array $array, int $flags): bool` | `bool` |
| [`shuffle()`](./array/shuffle.md) | `(array $array): bool` | `bool` |
| [`sort()`](./array/sort.md) | `(array $array, int $flags): bool` | `bool` |
| [`uasort()`](./array/uasort.md) | `(array $array, callable $callback): bool` | `bool` |
| [`uksort()`](./array/uksort.md) | `(array $array, callable $callback): bool` | `bool` |
| [`usort()`](./array/usort.md) | `(array $array, callable $callback): bool` | `bool` |
