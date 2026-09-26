---
title: "date_sub()"
description: "Subtracts an interval from a DateTime, modifying it in place."
sidebar:
  order: 210
---

## date_sub()

```php
function date_sub(mixed $object, mixed $interval): mixed
```

Subtracts an interval from a DateTime, modifying it in place.

**Parameters**:
- `$object` (`mixed`)
- `$interval` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_sub` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_sub.md).
