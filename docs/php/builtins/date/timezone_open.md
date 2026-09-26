---
title: "timezone_open()"
description: "Creates a DateTimeZone from an identifier."
sidebar:
  order: 252
---

## timezone_open()

```php
function timezone_open(string $timezone): mixed
```

Creates a DateTimeZone from an identifier.

**Parameters**:
- `$timezone` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `timezone_open` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/timezone_open.md).
