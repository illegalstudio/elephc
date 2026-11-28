---
title: "array_walk()"
description: "Applies a user function to every member of an array. A walk over a local array whose callback declares a writable value or reads the key promotes that array to boxed storage, then passes a writable value reference and key to visible native closures, named functions, first-class callables, and callable arrays. Escaping element references through a closure capture or property remains unsupported."
---

## array_walk()

```php
function array_walk(array $array, callable $callback): bool
```

Applies a user function to every member of an array. A walk over a local array whose callback declares a writable value or reads the key promotes that array to boxed storage, then passes a writable value reference and key to visible native closures, named functions, first-class callables, and callable arrays. Escaping element references through a closure capture or property remains unsupported.

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
