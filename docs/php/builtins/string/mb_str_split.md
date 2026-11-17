---
title: "mb_str_split()"
description: "Splits a string into chunks measured in encoded characters."
---

## mb_str_split()

```php
function mb_str_split(string $string, int $length = 1, ?string $encoding = null): array
```

Splits a string into chunks measured in encoded characters.

**Parameters**:
- `$string` (`string`)
- `$length` (`int`), default `1`, optional
- `$encoding` (`?string`), default `null`, optional

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_str_split` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_str_split.md).
