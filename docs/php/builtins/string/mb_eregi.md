---
title: "mb_eregi()"
description: "Searches without case sensitivity and optionally writes multibyte captures by reference."
---

## mb_eregi()

```php
function mb_eregi(string $pattern, string $string, mixed &$matches = null): bool
```

Searches without case sensitivity and optionally writes multibyte captures by reference.

**Parameters**:
- `$pattern` (`string`)
- `$string` (`string`)
- `$matches` (`mixed`), passed by reference, default `null`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_eregi` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_eregi.md).
