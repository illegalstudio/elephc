---
title: "intlcal_add()"
description: "PHP ext/intl `intlcal_add()` backed by the managed ICU bridge."
---

## intlcal_add()

```php
function intlcal_add(object $calendar, int $field, int $value): bool
```

PHP ext/intl `intlcal_add()` backed by the managed ICU bridge.

**Parameters**:
- `$calendar` (`object`)
- `$field` (`int`)
- `$value` (`int`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `intlcal_add` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/intlcal_add.md).
