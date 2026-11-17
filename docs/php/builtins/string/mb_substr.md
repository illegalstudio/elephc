---
title: "mb_substr()"
description: "Selects a substring using character offsets and an optional character length."
---

## mb_substr()

```php
function mb_substr(string $string, int $start, ?int $length = null, ?string $encoding = null): string
```

Selects a substring using character offsets and an optional character length.

**Parameters**:
- `$string` (`string`)
- `$start` (`int`)
- `$length` (`?int`), default `null`, optional
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_substr` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_substr.md).
