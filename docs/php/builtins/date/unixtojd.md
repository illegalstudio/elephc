---
title: "unixtojd()"
description: "Converts a Unix timestamp into a Julian Day count."
sidebar:
  order: 255
---

## unixtojd()

```php
function unixtojd(?int $timestamp = null): mixed
```

Converts a Unix timestamp into a Julian Day count.

**Parameters**:
- `$timestamp` (`?int`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `unixtojd` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/unixtojd.md).
