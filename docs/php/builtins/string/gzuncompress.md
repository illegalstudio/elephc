---
title: "gzuncompress()"
description: "Uncompress a compressed string."
---

## gzuncompress()

```php
function gzuncompress(string $data, int $max_length = 0): mixed
```

Uncompress a compressed string.

**Parameters**:
- `$data` (`string`)
- `$max_length` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gzuncompress` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/gzuncompress.md).
