---
title: "intlcal_create_instance()"
description: "PHP ext/intl `intlcal_create_instance()` backed by the managed ICU bridge."
---

## intlcal_create_instance()

```php
function intlcal_create_instance(object|string|null $timezone = null, ?string $locale = null): ?object
```

PHP ext/intl `intlcal_create_instance()` backed by the managed ICU bridge.

**Parameters**:
- `$timezone` (`object|string|null`), default `null`, optional
- `$locale` (`?string`), default `null`, optional

**Returns**: `?object`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `intlcal_create_instance` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/intlcal_create_instance.md).
