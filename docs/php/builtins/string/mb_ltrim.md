---
title: "mb_ltrim()"
description: "Removes Unicode whitespace or the specified characters from the beginning."
---

## mb_ltrim()

```php
function mb_ltrim(string $string, ?string $characters = null, ?string $encoding = null): string
```

Removes Unicode whitespace or the specified characters from the beginning.

**Parameters**:
- `$string` (`string`)
- `$characters` (`?string`), default `null`, optional
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_ltrim` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_ltrim.md).
