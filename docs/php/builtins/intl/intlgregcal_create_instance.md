---
title: "intlgregcal_create_instance()"
description: "PHP ext/intl `intlgregcal_create_instance()` backed by the managed ICU bridge."
---

## intlgregcal_create_instance()

```php
function intlgregcal_create_instance(mixed $timezoneOrYear = null, mixed $localeOrMonth = null, mixed $day = null, mixed $hour = null, mixed $minute = null, mixed $second = null): ?object
```

PHP ext/intl `intlgregcal_create_instance()` backed by the managed ICU bridge.

**Parameters**:
- `$timezoneOrYear` (`mixed`), default `null`, optional
- `$localeOrMonth` (`mixed`), default `null`, optional
- `$day` (`mixed`), default `null`, optional
- `$hour` (`mixed`), default `null`, optional
- `$minute` (`mixed`), default `null`, optional
- `$second` (`mixed`), default `null`, optional

**Returns**: `?object`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `intlgregcal_create_instance` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/intlgregcal_create_instance.md).
