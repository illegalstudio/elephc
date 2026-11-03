---
title: "ob_get_status()"
description: "Gets status of output buffers."
---

## ob_get_status()

```php
function ob_get_status(bool $full_status = false): array
```

Gets status of output buffers.

**Parameters**:
- `$full_status` (`bool`), default `false`, optional

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ob_get_status` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/ob_get_status.md).
