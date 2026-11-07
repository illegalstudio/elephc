---
title: "locale_get_display_script()"
description: "PHP ext/intl `locale_get_display_script()` backed by the managed ICU bridge."
---

## locale_get_display_script()

```php
function locale_get_display_script(string $locale, ?string $displayLocale = null): string|false
```

PHP ext/intl `locale_get_display_script()` backed by the managed ICU bridge.

**Parameters**:
- `$locale` (`string`)
- `$displayLocale` (`?string`), default `null`, optional

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `locale_get_display_script` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/locale_get_display_script.md).
