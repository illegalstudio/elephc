---
title: "jdtojulian()"
description: "Converts a Julian Day count into a Julian calendar date string."
sidebar:
  order: 235
---

## jdtojulian()

```php
function jdtojulian(int $julian_day): string
```

Converts a Julian Day count into a Julian calendar date string.

**Parameters**:
- `$julian_day` (`int`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `jdtojulian` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/jdtojulian.md).
