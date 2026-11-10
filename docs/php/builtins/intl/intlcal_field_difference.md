---
title: "intlcal_field_difference()"
description: "PHP ext/intl `intlcal_field_difference()` backed by the managed ICU bridge."
---

## intlcal_field_difference()

```php
function intlcal_field_difference(object $calendar, float $timestamp, int $field): int|false
```

PHP ext/intl `intlcal_field_difference()` backed by the managed ICU bridge.

**Parameters**:
- `$calendar` (`object`)
- `$timestamp` (`float`)
- `$field` (`int`)

**Returns**: `int|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `intlcal_field_difference` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/intlcal_field_difference.md).
