---
title: "date_sun_info()"
description: "Returns the sunrise, sunset, and twilight times for a day and location."
sidebar:
  order: 211
---

## date_sun_info()

```php
function date_sun_info(int $timestamp, float $latitude, float $longitude): array
```

Returns the sunrise, sunset, and twilight times for a day and location.

**Parameters**:
- `$timestamp` (`int`)
- `$latitude` (`float`)
- `$longitude` (`float`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_sun_info` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_sun_info.md).
