---
title: "ucwords()"
description: "Uppercases the first character of each word in a string."
---

## ucwords()

```php
function ucwords(string $string, string $separators = " \t\r\n\x0C\x0B"): string
```

Uppercases the first character of each word in a string.

**Parameters**:
- `$string` (`string`)
- `$separators` (`string`), default `" \t\r\n\x0C\x0B"`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ucwords` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/ucwords.md).
