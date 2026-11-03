---
title: "intlcal_set_time_zone()"
description: "PHP ext/intl `intlcal_set_time_zone()` backed by the managed ICU bridge."
---

## intlcal_set_time_zone()

```php
function intlcal_set_time_zone(object $calendar, object|string|null $timezone): bool
```

PHP ext/intl `intlcal_set_time_zone()` backed by the managed ICU bridge.

**Parameters**:
- `$calendar` (`object`)
- `$timezone` (`object|string|null`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `intlcal_set_time_zone` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/intlcal_set_time_zone.md).
