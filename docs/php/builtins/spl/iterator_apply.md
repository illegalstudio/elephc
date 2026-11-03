---
title: "iterator_apply()"
description: "Call a function for every element in an iterator."
---

## iterator_apply()

```php
function iterator_apply(traversable $iterator, callable $callback, array $args = null): int
```

Call a function for every element in an iterator.

**Parameters**:
- `$iterator` (`traversable`)
- `$callback` (`callable`)
- `$args` (`array`), default `null`, optional

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `iterator_apply` is implemented in the compiler, see [the internals page](../../../internals/builtins/spl/iterator_apply.md).
