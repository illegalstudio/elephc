---
title: "gmstrftime()"
description: "Formats a UTC timestamp with locale-aware strftime specifiers. Deprecated since PHP 8.1."
sidebar:
  order: 226
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

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmstrftime` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/gmstrftime.md).
