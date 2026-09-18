---
title: "array_walk_recursive()"
description: "Applies a user function recursively to array leaf values. AOT boxed arrays pass a writable leaf reference and key to visible native callbacks. Escaping element references and opaque callback descriptors are unsupported."
sidebar:
  order: 47
---

## array_walk_recursive()

```php
function array_walk_recursive(array $array, callable $callback): void
```

Applies a user function recursively to array leaf values. AOT boxed arrays pass a writable leaf reference and key to visible native callbacks. Escaping element references and opaque callback descriptors are unsupported.

**Parameters**:
- `$array` (`array`), passed by reference
- `$callback` (`callable`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_walk_recursive` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_walk_recursive.md).
