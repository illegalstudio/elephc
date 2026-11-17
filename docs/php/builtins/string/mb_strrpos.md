---
title: "mb_strrpos()"
description: "Finds the last character position of an encoded substring, or returns false."
---

## mb_strrpos()

```php
function mb_strrpos(string $haystack, string $needle, int $offset = 0, ?string $encoding = null): int|false
```

Finds the last character position of an encoded substring, or returns false.

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

For how `mb_strrpos` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_strrpos.md).
