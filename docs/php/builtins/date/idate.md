---
title: "idate()"
description: "Formats a timestamp with a single format character and returns it as an integer."
---

## idate()

```php
function idate(string $format, ?int $timestamp = null): mixed
```

Formats a timestamp with a single format character and returns it as an integer.

**Parameters**:
- `$format` (`string`)
- `$timestamp` (`?int`), default `null`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `idate` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/idate.md).
