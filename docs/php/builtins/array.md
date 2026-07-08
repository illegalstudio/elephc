---
title: "Array builtins"
description: "Builtins in the Array category."
sidebar:
  order: 102
---

## Array builtins

| Function | Signature | Returns |
|---|---|---|
| [`array_all()`](./array/array_all.md) | `(mixed $array, mixed $callback): bool` | `bool` |
| [`array_any()`](./array/array_any.md) | `(mixed $array, mixed $callback): bool` | `bool` |
| [`array_chunk()`](./array/array_chunk.md) | `(array $array, int $length): array` | `array` |
| [`array_column()`](./array/array_column.md) | `(array $array, string $column_key): array` | `array` |
| [`array_combine()`](./array/array_combine.md) | `(array $keys, array $values): array` | `array` |
| [`array_diff()`](./array/array_diff.md) | `(array $array, ...$arrays): array` | `array` |
| [`array_diff_assoc()`](./array/array_diff_assoc.md) | `(array $array, ...$arrays): mixed` | `mixed` |
| [`array_diff_key()`](./array/array_diff_key.md) | `(array $array, ...$arrays): array` | `array` |
| [`array_fill()`](./array/array_fill.md) | `(int $start_index, int $count, mixed $value): array` | `array` |
| [`array_fill_keys()`](./array/array_fill_keys.md) | `(array $keys, mixed $value): array` | `array` |
| [`array_filter()`](./array/array_filter.md) | `(array $array, callable $callback = null, int $mode = 0): array` | `array` |
| [`array_find()`](./array/array_find.md) | `(mixed $array, mixed $callback): mixed` | `mixed` |
| [`array_flip()`](./array/array_flip.md) | `(array $array): array` | `array` |
| [`array_intersect()`](./array/array_intersect.md) | `(array $array, ...$arrays): array` | `array` |
| [`array_intersect_assoc()`](./array/array_intersect_assoc.md) | `(array $array, ...$arrays): mixed` | `mixed` |
| [`array_intersect_key()`](./array/array_intersect_key.md) | `(array $array, ...$arrays): array` | `array` |
| [`array_is_list()`](./array/array_is_list.md) | `(mixed $array): bool` | `bool` |
| [`array_key_exists()`](./array/array_key_exists.md) | `(string $key, array $array): bool` | `bool` |
| [`array_key_first()`](./array/array_key_first.md) | `(array $array): mixed` | `mixed` |
| [`array_key_last()`](./array/array_key_last.md) | `(array $array): mixed` | `mixed` |
| [`array_keys()`](./array/array_keys.md) | `(array $array): array` | `array` |
| [`array_map()`](./array/array_map.md) | `(callable $callback, array $array, ...$arrays): array` | `array` |
| [`array_merge()`](./array/array_merge.md) | `(...$arrays): array` | `array` |
| [`array_merge_recursive()`](./array/array_merge_recursive.md) | `(...$arrays): array` | `array` |
| [`array_multisort()`](./array/array_multisort.md) | `(array $array1, int $array2): bool` | `bool` |
| [`array_pad()`](./array/array_pad.md) | `(array $array, int $length, mixed $value): array` | `array` |
| [`array_pop()`](./array/array_pop.md) | `(array $array): mixed` | `mixed` |
| [`array_product()`](./array/array_product.md) | `(array $array): int` | `int` |
| [`array_push()`](./array/array_push.md) | `(array $array, ...$values): void` | `void` |
| [`array_rand()`](./array/array_rand.md) | `(array $array): int` | `int` |
| [`array_reduce()`](./array/array_reduce.md) | `(array $array, callable $callback, mixed $initial = null): int` | `int` |
| [`array_replace()`](./array/array_replace.md) | `(array $array, array $replacements): mixed` | `mixed` |
| [`array_replace_recursive()`](./array/array_replace_recursive.md) | `(array $array, array $replacements): mixed` | `mixed` |
| [`array_reverse()`](./array/array_reverse.md) | `(array $array): array` | `array` |
| [`array_search()`](./array/array_search.md) | `(mixed $needle, array $haystack, bool $strict = false): mixed` | `mixed` |
| [`array_shift()`](./array/array_shift.md) | `(array $array): mixed` | `mixed` |
| [`array_slice()`](./array/array_slice.md) | `(array $array, int $offset, int $length = null): array` | `array` |
| [`array_splice()`](./array/array_splice.md) | `(array $array, int $offset, int $length = null): array` | `array` |
| [`array_sum()`](./array/array_sum.md) | `(array $array): int` | `int` |
| [`array_udiff()`](./array/array_udiff.md) | `(array $array1, array $array2, callable $callback): array` | `array` |
| [`array_uintersect()`](./array/array_uintersect.md) | `(array $array1, array $array2, callable $callback): array` | `array` |
| [`array_unique()`](./array/array_unique.md) | `(array $array): array` | `array` |
| [`array_unshift()`](./array/array_unshift.md) | `(array $array, ...$values): int` | `int` |
| [`array_values()`](./array/array_values.md) | `(array $array): array` | `array` |
| [`array_walk()`](./array/array_walk.md) | `(array $array, callable $callback): void` | `void` |
| [`array_walk_recursive()`](./array/array_walk_recursive.md) | `(array $array, callable $callback): void` | `void` |
| [`arsort()`](./array/arsort.md) | `(array $array): bool` | `bool` |
| [`asort()`](./array/asort.md) | `(array $array): bool` | `bool` |
| [`call_user_func()`](./array/call_user_func.md) | `(callable $callback, ...$args): mixed` | `mixed` |
| [`call_user_func_array()`](./array/call_user_func_array.md) | `(callable $callback, array $args): mixed` | `mixed` |
| [`count()`](./array/count.md) | `(array $value, int $mode = 0): int` | `int` |
| [`in_array()`](./array/in_array.md) | `(mixed $needle, array $haystack, bool $strict = false): bool` | `bool` |
| [`krsort()`](./array/krsort.md) | `(array $array): bool` | `bool` |
| [`ksort()`](./array/ksort.md) | `(array $array): bool` | `bool` |
| [`natcasesort()`](./array/natcasesort.md) | `(array $array): bool` | `bool` |
| [`natsort()`](./array/natsort.md) | `(array $array): bool` | `bool` |
| [`range()`](./array/range.md) | `(mixed $start, mixed $end): array` | `array` |
| [`rsort()`](./array/rsort.md) | `(array $array): bool` | `bool` |
| [`shuffle()`](./array/shuffle.md) | `(array $array): bool` | `bool` |
| [`sort()`](./array/sort.md) | `(array $array): bool` | `bool` |
| [`uasort()`](./array/uasort.md) | `(array $array, callable $callback): bool` | `bool` |
| [`uksort()`](./array/uksort.md) | `(array $array, callable $callback): bool` | `bool` |
| [`usort()`](./array/usort.md) | `(array $array, callable $callback): bool` | `bool` |
