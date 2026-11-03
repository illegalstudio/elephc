---
title: "timezone_identifiers_list()"
description: "Returns the timezone identifiers this build knows, optionally filtered."
---

## timezone_identifiers_list()

```php
function timezone_identifiers_list(int $timezoneGroup = DateTimeZone::ALL, ?string $countryCode = null): array
```

Returns the timezone identifiers this build knows, optionally filtered.

**Parameters**:
- `$timezoneGroup` (`int`), default `DateTimeZone::ALL`, optional
- `$countryCode` (`?string`), default `null`, optional

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `timezone_identifiers_list` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/timezone_identifiers_list.md).
