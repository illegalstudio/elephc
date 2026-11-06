---
title: "is_callable()"
description: "Checks whether a variable can be called as a function."
---

## is_callable()

```php
function is_callable(mixed $value): bool
```

Checks whether a variable can be called as a function.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `is_callable` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/is_callable.md).
