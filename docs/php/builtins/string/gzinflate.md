---
title: "gzinflate()"
description: "Inflate a deflated string."
---

## gzinflate()

```php
function gzinflate(string $data, int $max_length = 0): mixed
```

Inflate a deflated string.

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

For how `gzinflate` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/gzinflate.md).
