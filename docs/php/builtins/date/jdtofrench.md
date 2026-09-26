---
title: "jdtofrench()"
description: "Converts a Julian Day count into a French Republican date string."
sidebar:
  order: 232
---

## jdtofrench()

```php
function jdtofrench(int $julian_day): string
```

Converts a Julian Day count into a French Republican date string.

**Parameters**:
- `$julian_day` (`int`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `jdtofrench` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/jdtofrench.md).
