---
title: "mb_detect_order()"
description: "Reads or updates the request's encoding detection order."
---

## mb_detect_order()

```php
function mb_detect_order(array|string|null $encoding = null): array|bool
```

Reads or updates the request's encoding detection order.

**Parameters**:
- `$encoding` (`array|string|null`), default `null`, optional

**Returns**: `array|bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_detect_order` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_detect_order.md).
