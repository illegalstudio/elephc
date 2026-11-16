---
title: "mb_trim()"
description: "Removes Unicode whitespace or the specified characters from both ends."
---

## mb_trim()

```php
function mb_trim(string $string, ?string $characters = null, ?string $encoding = null): string
```

Removes Unicode whitespace or the specified characters from both ends.

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

For how `mb_trim` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_trim.md).
