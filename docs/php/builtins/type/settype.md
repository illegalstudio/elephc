---
title: "settype()"
description: "Sets the type of a variable."
---

## settype()

```php
function settype(mixed &$var, string $type): bool
```

Sets the type of a variable.

**Parameters**:
- `$var` (`mixed`), passed by reference
- `$type` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `settype` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/settype.md).
