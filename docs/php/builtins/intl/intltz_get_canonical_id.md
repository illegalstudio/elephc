---
title: "intltz_get_canonical_id()"
description: "PHP ext/intl `intltz_get_canonical_id()` backed by the managed ICU bridge."
---

## intltz_get_canonical_id()

```php
function intltz_get_canonical_id(string $timezoneId, mixed $isSystemId = null): string|false
```

PHP ext/intl `intltz_get_canonical_id()` backed by the managed ICU bridge.

**Parameters**:
- `$timezoneId` (`string`)
- `$isSystemId` (`mixed`), passed by reference, default `null`, optional

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `intltz_get_canonical_id` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/intltz_get_canonical_id.md).
