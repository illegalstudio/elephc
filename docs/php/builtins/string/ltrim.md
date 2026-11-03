---
title: "ltrim()"
description: "Strips whitespace (or other characters) from the beginning of a string."
---

## ltrim()

```php
function ltrim(string $string, string $characters = " \n\r\t\x0B\x0C\x00"): string
```

Strips whitespace (or other characters) from the beginning of a string.

**Parameters**:
- `$string` (`string`)
- `$characters` (`string`), default `" \n\r\t\x0B\x0C\x00"`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ltrim` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/ltrim.md).
