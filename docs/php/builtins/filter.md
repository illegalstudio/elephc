---
title: "Filter builtins"
description: "Builtins in the Filter category."
sidebar:
  order: 124
---

## Filter builtins

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`filter_has_var()`](./filter/filter_has_var.md) | `(int $input_type, string $var_name): bool` | `bool` | ✓ | ✓ |
| [`filter_id()`](./filter/filter_id.md) | `(string $name): int|false` | `int|false` | ✓ | ✓ |
| [`filter_input()`](./filter/filter_input.md) | `(int $type, string $var_name, int $filter = FILTER_DEFAULT, array|int $options = 0): mixed` | `mixed` | ✓ | ✓ |
| [`filter_input_array()`](./filter/filter_input_array.md) | `(int $type, array|int $options = FILTER_DEFAULT, bool $add_empty = true): array|false|null` | `array|false|null` | ✓ | ✓ |
| [`filter_list()`](./filter/filter_list.md) | `(): array` | `array` | ✓ | ✓ |
| [`filter_var()`](./filter/filter_var.md) | `(mixed $value, int $filter = FILTER_DEFAULT, array|int $options = 0): mixed` | `mixed` | ✓ | ✓ |
| [`filter_var_array()`](./filter/filter_var_array.md) | `(array $array, array|int $options = FILTER_DEFAULT, bool $add_empty = true): array|false|null` | `array|false|null` | ✓ | ✓ |
