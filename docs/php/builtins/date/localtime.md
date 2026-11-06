---
title: "localtime()"
description: "Returns the local time."
---

## localtime()

```php
function localtime(int $timestamp = -1, bool $associative = false): array
```

Returns the local time.

**Parameters**:
- `$timestamp` (`int`), default `-1`, optional
- `$associative` (`bool`), default `false`, optional

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `localtime` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/localtime.md).
