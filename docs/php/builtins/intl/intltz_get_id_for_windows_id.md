---
title: "intltz_get_id_for_windows_id()"
description: "PHP ext/intl `intltz_get_id_for_windows_id()` backed by the managed ICU bridge."
---

## intltz_get_id_for_windows_id()

```php
function intltz_get_id_for_windows_id(string $timezoneId, ?string $region = null): string|false
```

PHP ext/intl `intltz_get_id_for_windows_id()` backed by the managed ICU bridge.

**Parameters**:
- `$timezoneId` (`string`)
- `$region` (`?string`), default `null`, optional

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `intltz_get_id_for_windows_id` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/intltz_get_id_for_windows_id.md).
