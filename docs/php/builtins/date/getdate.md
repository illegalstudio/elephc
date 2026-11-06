---
title: "getdate()"
description: "Returns date/time information."
---

## getdate()

```php
function getdate(?int $timestamp = null): array
```

Returns date/time information.

**Parameters**:
- `$timestamp` (`?int`), default `null`, optional

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `getdate` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/getdate.md).
