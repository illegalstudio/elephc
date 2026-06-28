---
title: "SPL builtins"
description: "Builtins in the SPL category."
sidebar:
  order: 114
---

## SPL builtins

| Function | Signature | Returns |
|---|---|---|
| [`iterator_apply()`](./spl/iterator_apply.md) | `(traversable $iterator, callable $callback, array $args): int` | `int` |
| [`iterator_count()`](./spl/iterator_count.md) | `(traversable $iterator): int` | `int` |
| [`iterator_to_array()`](./spl/iterator_to_array.md) | `(traversable $iterator, bool $preserve_keys): array` | `array` |
| [`spl_autoload()`](./spl/spl_autoload.md) | `(string $class, string $file_extensions): void` | `void` |
| [`spl_autoload_call()`](./spl/spl_autoload_call.md) | `(string $class): void` | `void` |
| [`spl_autoload_extensions()`](./spl/spl_autoload_extensions.md) | `(string $file_extensions): string` | `string` |
| [`spl_autoload_functions()`](./spl/spl_autoload_functions.md) | `(): array` | `array` |
| [`spl_autoload_register()`](./spl/spl_autoload_register.md) | `(callable $callback, bool $throw, bool $prepend): bool` | `bool` |
| [`spl_autoload_unregister()`](./spl/spl_autoload_unregister.md) | `(callable $callback): bool` | `bool` |
| [`spl_classes()`](./spl/spl_classes.md) | `(): array` | `array` |
| [`spl_object_hash()`](./spl/spl_object_hash.md) | `(object $object): string` | `string` |
| [`spl_object_id()`](./spl/spl_object_id.md) | `(object $object): int` | `int` |
