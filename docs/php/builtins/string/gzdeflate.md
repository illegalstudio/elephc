---
title: "gzdeflate()"
description: "Deflate a string using the DEFLATE data format."
---

## gzdeflate()

```php
function gzdeflate(string $data, int $level = -1): string
```

Deflate a string using the DEFLATE data format.

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

For how `gzdeflate` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/gzdeflate.md).
