---
title: "gmstrftime()"
description: "Formats a UTC timestamp with locale-aware strftime specifiers. Deprecated since PHP 8.1."
---

## gmstrftime()

```php
function gmstrftime(string $format, ?int $timestamp = null): mixed
```

Formats a UTC timestamp with locale-aware strftime specifiers. Deprecated since PHP 8.1.

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

For how `gmstrftime` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/gmstrftime.md).
