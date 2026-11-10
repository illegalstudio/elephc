---
title: "locale_lookup()"
description: "PHP ext/intl `locale_lookup()` backed by the managed ICU bridge."
---

## locale_lookup()

```php
function locale_lookup(array $languageTag, string $locale, bool $canonicalize = false, ?string $defaultLocale = null): ?string
```

PHP ext/intl `locale_lookup()` backed by the managed ICU bridge.

**Parameters**:
- `$languageTag` (`array`)
- `$locale` (`string`)
- `$canonicalize` (`bool`), default `false`, optional
- `$defaultLocale` (`?string`), default `null`, optional

**Returns**: `?string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `locale_lookup` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/locale_lookup.md).
