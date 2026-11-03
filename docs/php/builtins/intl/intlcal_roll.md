---
title: "intlcal_roll()"
description: "PHP ext/intl `intlcal_roll()` backed by the managed ICU bridge."
---

## intlcal_roll()

```php
function intlcal_roll(object $calendar, int $field, mixed $value): bool
```

PHP ext/intl `intlcal_roll()` backed by the managed ICU bridge.

**Parameters**:
- `$calendar` (`object`)
- `$field` (`int`)
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `intlcal_roll` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/intlcal_roll.md).
