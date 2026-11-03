---
title: "gzcompress()"
description: "Compress a string using the ZLIB data format."
---

## gzcompress()

```php
function gzcompress(string $data, int $level = -1): string
```

Compress a string using the ZLIB data format.

**Parameters**:
- `$data` (`string`)
- `$level` (`int`), default `-1`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gzcompress` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/gzcompress.md).
