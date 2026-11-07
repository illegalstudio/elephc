---
title: "intlcal_from_date_time()"
description: "PHP ext/intl `intlcal_from_date_time()` backed by the managed ICU bridge."
---

## intlcal_from_date_time()

```php
function intlcal_from_date_time(object|string $datetime, ?string $locale = null): ?object
```

PHP ext/intl `intlcal_from_date_time()` backed by the managed ICU bridge.

**Parameters**:
- `$datetime` (`object|string`)
- `$locale` (`?string`), default `null`, optional

**Returns**: `?object`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `intlcal_from_date_time` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/intlcal_from_date_time.md).
