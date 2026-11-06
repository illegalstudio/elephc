---
title: "mb_strrichr()"
description: "Returns text before or from the last case-insensitive substring match, or false."
---

## mb_strrichr()

```php
function mb_strrichr(string $haystack, string $needle, bool $before_needle = false, ?string $encoding = null): string|false
```

Returns text before or from the last case-insensitive substring match, or false.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)
- `$before_needle` (`bool`), default `false`, optional
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_strrichr` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_strrichr.md).
