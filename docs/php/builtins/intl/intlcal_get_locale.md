---
title: "intlcal_get_locale()"
description: "PHP ext/intl `intlcal_get_locale()` backed by the managed ICU bridge."
---

## intlcal_get_locale()

```php
function intlcal_get_locale(object $calendar, int $type): string|false
```

PHP ext/intl `intlcal_get_locale()` backed by the managed ICU bridge.

**Parameters**:
- `$calendar` (`object`)
- `$type` (`int`)

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `intlcal_get_locale` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/intlcal_get_locale.md).
