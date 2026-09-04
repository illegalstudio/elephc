---
title: "date_sunrise()"
description: "Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking."
sidebar:
  order: 209
---

## date_sunrise()

```php
function date_sunrise(int $timestamp, int $returnFormat = SUNFUNCS_RET_STRING, float $latitude = null, float $longitude = null, float $zenith = null, float $utcOffset = null): mixed
```

Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

**Parameters**:
- `$timestamp` (`int`)
- `$returnFormat` (`int`), default `SUNFUNCS_RET_STRING`, optional
- `$latitude` (`float`), default `null`, optional
- `$longitude` (`float`), default `null`, optional
- `$zenith` (`float`), default `null`, optional
- `$utcOffset` (`float`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_sunrise` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_sunrise.md).
