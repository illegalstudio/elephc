---
title: "mb_stripos()"
description: "Finds the first character position using simple case folding, or returns false."
---

## mb_stripos()

```php
function mb_stripos(string $haystack, string $needle, int $offset = 0, ?string $encoding = null): int|false
```

Finds the first character position using simple case folding, or returns false.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)
- `$offset` (`int`), default `0`, optional
- `$encoding` (`?string`), default `null`, optional

**Returns**: `int|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_stripos` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_stripos.md).
