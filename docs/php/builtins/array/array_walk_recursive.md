---
title: "array_walk_recursive()"
description: "Applies a user function recursively to array leaf values. AOT boxed arrays pass a writable leaf reference and key to visible native callbacks. Escaping element references and opaque callback descriptors are unsupported."
sidebar:
  order: 59
---

## array_walk_recursive()

```php
function array_walk_recursive(array $array, callable $callback, mixed $arg = null): bool
```

Applies a user function recursively to array leaf values. AOT boxed arrays pass a writable leaf reference and key to visible native callbacks. Escaping element references and opaque callback descriptors are unsupported.

**Parameters**:
- `$array` (`array`), passed by reference
- `$callback` (`callable`)
- `$arg` (`mixed`), default `null`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_walk_recursive.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_walk_recursive.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_walk_recursive` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_walk_recursive.md).
