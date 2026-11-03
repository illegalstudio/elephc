---
title: "locale_filter_matches()"
description: "PHP ext/intl `locale_filter_matches()` backed by the managed ICU bridge."
---

## locale_filter_matches()

```php
function locale_filter_matches(string $languageTag, string $locale, bool $canonicalize = false): ?bool
```

PHP ext/intl `locale_filter_matches()` backed by the managed ICU bridge.

**Parameters**:
- `$languageTag` (`string`)
- `$locale` (`string`)
- `$canonicalize` (`bool`), default `false`, optional

**Returns**: `?bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `locale_filter_matches` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/locale_filter_matches.md).
