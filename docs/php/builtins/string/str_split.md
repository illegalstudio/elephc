---
title: "str_split()"
description: "Converts a string into an array of chunks of the given length."
---

## str_split()

```php
function str_split(string $string, int $length = 1): array
```

Converts a string into an array of chunks of the given length.

**Parameters**:
- `$string` (`string`)
- `$length` (`int`), default `1`, optional

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `str_split` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/str_split.md).
