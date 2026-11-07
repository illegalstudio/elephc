---
title: "intlcal_is_weekend()"
description: "PHP ext/intl `intlcal_is_weekend()` backed by the managed ICU bridge."
---

## intlcal_is_weekend()

```php
function intlcal_is_weekend(object $calendar, ?float $timestamp = null): bool
```

PHP ext/intl `intlcal_is_weekend()` backed by the managed ICU bridge.

**Parameters**:
- `$calendar` (`object`)
- `$timestamp` (`?float`), default `null`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `intlcal_is_weekend` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/intlcal_is_weekend.md).
