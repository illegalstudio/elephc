---
title: "mb_ereg()"
description: "Searches a multibyte string and optionally writes numeric and named captures by reference."
---

## mb_ereg()

```php
function mb_ereg(string $pattern, string $string, mixed $matches = null): bool
```

Searches a multibyte string and optionally writes numeric and named captures by reference.

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

For how `mb_ereg` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_ereg.md).
