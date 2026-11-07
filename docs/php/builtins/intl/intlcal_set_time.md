---
title: "intlcal_set_time()"
description: "PHP ext/intl `intlcal_set_time()` backed by the managed ICU bridge."
---

## intlcal_set_time()

```php
function intlcal_set_time(object $calendar, float $timestamp): bool
```

PHP ext/intl `intlcal_set_time()` backed by the managed ICU bridge.

**Parameters**:
- `$calendar` (`object`)
- `$timestamp` (`float`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `intlcal_set_time` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/intlcal_set_time.md).
