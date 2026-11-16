---
title: "chop()"
description: "Alias of rtrim: strips whitespace (or other characters) from the end of a string."
---

## chop()

```php
function chop(string $string, string $characters = " \n\r\t\x0B\x0C\x00"): string
```

Alias of rtrim: strips whitespace (or other characters) from the end of a string.

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

For how `chop` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/chop.md).
