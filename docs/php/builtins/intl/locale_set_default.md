---
title: "locale_set_default()"
description: "PHP ext/intl `locale_set_default()` backed by the managed ICU bridge."
---

## locale_set_default()

```php
function locale_set_default(string $locale): bool
```

PHP ext/intl `locale_set_default()` backed by the managed ICU bridge.

**Parameters**:
- `$locale` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `locale_set_default` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/locale_set_default.md).
