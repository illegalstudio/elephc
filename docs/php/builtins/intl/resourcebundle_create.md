---
title: "resourcebundle_create()"
description: "PHP ext/intl `resourcebundle_create()` backed by the managed ICU bridge."
---

## resourcebundle_create()

```php
function resourcebundle_create(?string $locale, ?string $bundle, bool $fallback = true): ?object
```

PHP ext/intl `resourcebundle_create()` backed by the managed ICU bridge.

**Parameters**:
- `$locale` (`?string`)
- `$bundle` (`?string`)
- `$fallback` (`bool`), default `true`, optional

**Returns**: `?object`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `resourcebundle_create` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/resourcebundle_create.md).
