---
title: "intltz_get_display_name()"
description: "PHP ext/intl `intltz_get_display_name()` backed by the managed ICU bridge."
---

## intltz_get_display_name()

```php
function intltz_get_display_name(object $timezone, bool $dst = false, int $style = IntlTimeZone::DISPLAY_LONG, ?string $locale = null): string|false
```

PHP ext/intl `intltz_get_display_name()` backed by the managed ICU bridge.

**Parameters**:
- `$timezone` (`object`)
- `$dst` (`bool`), default `false`, optional
- `$style` (`int`), default `IntlTimeZone::DISPLAY_LONG`, optional
- `$locale` (`?string`), default `null`, optional

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `intltz_get_display_name` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/intltz_get_display_name.md).
