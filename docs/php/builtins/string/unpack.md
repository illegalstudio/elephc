---
title: "unpack()"
description: "Unpacks data from a binary string."
---

## unpack()

```php
function unpack(string $format, string $string, int $offset = 0): array|false
```

Unpacks data from a binary string.

**Parameters**:
- `$format` (`string`)
- `$string` (`string`)
- `$offset` (`int`), default `0`, optional

**Returns**: `array|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `unpack` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/unpack.md).
