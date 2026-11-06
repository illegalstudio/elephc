---
title: "mb_strwidth()"
description: "Returns the display width of a string in the requested encoding."
---

## mb_strwidth()

```php
function mb_strwidth(string $string, ?string $encoding = null): int
```

Returns the display width of a string in the requested encoding.

**Parameters**:
- `$string` (`string`)
- `$encoding` (`?string`), default `null`, optional

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_strwidth` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_strwidth.md).
