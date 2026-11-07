---
title: "locale_canonicalize()"
description: "PHP ext/intl `locale_canonicalize()` backed by the managed ICU bridge."
---

## locale_canonicalize()

```php
function locale_canonicalize(string $locale): ?string
```

PHP ext/intl `locale_canonicalize()` backed by the managed ICU bridge.

**Parameters**:
- `$locale` (`string`)

**Returns**: `?string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `locale_canonicalize` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/locale_canonicalize.md).
