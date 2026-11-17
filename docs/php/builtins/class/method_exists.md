---
title: "method_exists()"
description: "Checks whether a class method exists."
---

## method_exists()

```php
function method_exists(mixed $object_or_class, string $method): bool
```

Checks whether a class method exists.

**Parameters**:
- `$object_or_class` (`mixed`)
- `$method` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `method_exists` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/method_exists.md).
