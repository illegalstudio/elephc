---
title: "chunk_split()"
description: "Splits a string into fixed-length chunks separated by a given string."
---

## chunk_split()

```php
function chunk_split(string $string, int $length = 76, string $separator = "\r\n"): string
```

Splits a string into fixed-length chunks separated by a given string.

**Parameters**:
- `$string` (`string`)
- `$length` (`int`), default `76`, optional
- `$separator` (`string`), default `"\r\n"`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `chunk_split` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/chunk_split.md).
