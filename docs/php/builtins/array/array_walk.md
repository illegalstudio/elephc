---
title: "array_walk()"
description: "Applies a user function to every member of an array. AOT boxed arrays pass a writable value reference and key to visible native callbacks. Escaping element references and opaque callback descriptors are unsupported."
---

## array_walk()

```php
function array_walk(array &$array, callable $callback): bool
```

Applies a user function to every member of an array. AOT boxed arrays pass a writable value reference and key to visible native callbacks. Escaping element references and opaque callback descriptors are unsupported.

**Parameters**:
- `$array` (`array`), passed by reference
- `$callback` (`callable`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_walk` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_walk.md).
