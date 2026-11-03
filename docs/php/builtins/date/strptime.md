---
title: "strptime()"
description: "Parses a time string against a strftime format. Deprecated since PHP 8.1."
---

## strptime()

```php
function strptime(string $timestamp, string $format): mixed
```

Parses a time string against a strftime format. Deprecated since PHP 8.1.

**Parameters**:
- `$timestamp` (`string`)
- `$format` (`string`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `strptime` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/strptime.md).
