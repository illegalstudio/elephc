---
title: "hrtime()"
description: "Returns the current high-resolution time."
---

## hrtime()

```php
function hrtime(bool $as_number = false): mixed
```

Returns the current high-resolution time.

**Parameters**:
- `$as_number` (`bool`), default `false`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `hrtime` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/hrtime.md).
