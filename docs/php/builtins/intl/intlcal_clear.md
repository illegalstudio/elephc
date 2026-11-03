---
title: "intlcal_clear()"
description: "PHP ext/intl `intlcal_clear()` backed by the managed ICU bridge."
---

## intlcal_clear()

```php
function intlcal_clear(object $calendar, ?int $field = null): bool
```

PHP ext/intl `intlcal_clear()` backed by the managed ICU bridge.

**Parameters**:
- `$calendar` (`object`)
- `$field` (`?int`), default `null`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `intlcal_clear` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/intlcal_clear.md).
