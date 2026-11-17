---
title: "htmlentities()"
description: "Converts all applicable characters in a string into their HTML entities."
---

## htmlentities()

```php
function htmlentities(string $string, int $flags = 11, string $encoding = 'UTF-8'): string
```

Converts all applicable characters in a string into their HTML entities.

**Parameters**:
- `$string` (`string`)
- `$flags` (`int`), default `11`, optional
- `$encoding` (`string`), default `'UTF-8'`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `htmlentities` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/htmlentities.md).
