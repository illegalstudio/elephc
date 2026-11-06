---
title: "crc32()"
description: "Calculates the CRC32 polynomial of a string."
---

## crc32()

```php
function crc32(string $string): int
```

Calculates the CRC32 polynomial of a string.

**Parameters**:
- `$string` (`string`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `crc32` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/crc32.md).
