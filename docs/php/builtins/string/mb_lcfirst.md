---
title: "mb_lcfirst()"
description: "Converts the first character to Unicode lowercase."
---

## mb_lcfirst()

```php
function mb_lcfirst(string $string, ?string $encoding = null): string
```

Converts the first character to Unicode lowercase.

**Parameters**:
- `$string` (`string`)
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_lcfirst` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_lcfirst.md).
