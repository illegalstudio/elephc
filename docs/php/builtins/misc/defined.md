---
title: "defined()"
description: "Checks whether a given named constant exists."
---

## defined()

```php
function defined(string $constant_name): bool
```

Checks whether a given named constant exists.

**Parameters**:
- `$constant_name` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `defined` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/defined.md).
