---
title: "mb_split()"
description: "Splits a string using a multibyte regex and an optional maximum number of fields."
---

## mb_split()

```php
function mb_split(string $pattern, string $string, int $limit = -1): array|false
```

Splits a string using a multibyte regex and an optional maximum number of fields.

**Parameters**:
- `$pattern` (`string`)
- `$string` (`string`)
- `$limit` (`int`), default `-1`, optional

**Returns**: `array|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_split` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_split.md).
