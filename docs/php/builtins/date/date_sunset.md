---
title: "date_sunset()"
description: "Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking."
sidebar:
  order: 210
---

## date_sunset()

```php
function date_sunset(int $timestamp, int $returnFormat = SUNFUNCS_RET_STRING, ?float $latitude = null, ?float $longitude = null, ?float $zenith = null, ?float $utcOffset = null): mixed
```

Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

**Parameters**:
- `$timestamp` (`int`)
- `$returnFormat` (`int`), default `SUNFUNCS_RET_STRING`, optional
- `$latitude` (`?float`), default `null`, optional
- `$longitude` (`?float`), default `null`, optional
- `$zenith` (`?float`), default `null`, optional
- `$utcOffset` (`?float`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_sunset` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_sunset.md).
