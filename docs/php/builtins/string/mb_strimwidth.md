---
title: "mb_strimwidth()"
description: "Trims a string to a display width and appends a marker when needed."
---

## mb_strimwidth()

```php
function mb_strimwidth(string $string, int $start, int $width, string $trim_marker = '', ?string $encoding = null): string
```

Trims a string to a display width and appends a marker when needed.

**Parameters**:
- `$string` (`string`)
- `$start` (`int`)
- `$width` (`int`)
- `$trim_marker` (`string`), default `''`, optional
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_strimwidth` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_strimwidth.md).
