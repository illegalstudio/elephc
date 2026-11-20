---
title: "array_walk_recursive()"
description: "Applies a user function recursively to array leaf values. A walk over a local array whose callback declares a writable leaf or reads the key promotes that array to boxed storage, then passes a writable leaf reference and key to visible native closures, named functions, first-class callables, and callable arrays. Escaping element references through a closure capture or property remains unsupported."
---

## array_walk_recursive()

```php
function array_walk_recursive(array $array, callable $callback): bool
```

Applies a user function recursively to array leaf values. A walk over a local array whose callback declares a writable leaf or reads the key promotes that array to boxed storage, then passes a writable leaf reference and key to visible native closures, named functions, first-class callables, and callable arrays. Escaping element references through a closure capture or property remains unsupported.

**Parameters**:
- `$array` (`array`), passed by reference
- `$callback` (`callable`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_walk_recursive` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_walk_recursive.md).
