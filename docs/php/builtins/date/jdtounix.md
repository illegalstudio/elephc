---
title: "jdtounix()"
description: "Converts a Julian Day count into a Unix timestamp."
sidebar:
  order: 236
---

## jdtounix()

```php
function jdtounix(int $julian_day): int
```

Converts a Julian Day count into a Unix timestamp.

**Parameters**:
- `$julian_day` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `jdtounix` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/jdtounix.md).
