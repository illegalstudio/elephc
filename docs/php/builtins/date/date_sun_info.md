---
title: "date_sun_info()"
description: "Returns the sunrise, sunset, and twilight times for a day and location."
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

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_sun_info` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_sun_info.md).
