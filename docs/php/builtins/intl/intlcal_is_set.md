---
title: "intlcal_is_set()"
description: "PHP ext/intl `intlcal_is_set()` backed by the managed ICU bridge."
---

## intlcal_is_set()

```php
function intlcal_is_set(object $calendar, int $field): bool
```

PHP ext/intl `intlcal_is_set()` backed by the managed ICU bridge.

**Parameters**:
- `$calendar` (`object`)
- `$field` (`int`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `intlcal_is_set` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/intlcal_is_set.md).
