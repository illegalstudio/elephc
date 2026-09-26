---
title: "timezone_identifiers_list()"
description: "Returns the timezone identifiers this build knows, optionally filtered."
sidebar:
  order: 247
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

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `timezone_identifiers_list` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/timezone_identifiers_list.md).
