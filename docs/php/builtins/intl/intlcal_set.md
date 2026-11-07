---
title: "intlcal_set()"
description: "PHP ext/intl `intlcal_set()` backed by the managed ICU bridge."
---

## intlcal_set()

```php
function intlcal_set(object $calendar, int $year, int $month, int $dayOfMonth = null, int $hour = null, int $minute = null, int $second = null): bool
```

PHP ext/intl `intlcal_set()` backed by the managed ICU bridge.

**Parameters**:
- `$calendar` (`object`)
- `$year` (`int`)
- `$month` (`int`)
- `$dayOfMonth` (`int`), default `null`, optional
- `$hour` (`int`), default `null`, optional
- `$minute` (`int`), default `null`, optional
- `$second` (`int`), default `null`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `intlcal_set` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/intlcal_set.md).
