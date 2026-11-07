---
title: "resourcebundle_get()"
description: "PHP ext/intl `resourcebundle_get()` backed by the managed ICU bridge."
---

## resourcebundle_get()

```php
function resourcebundle_get(object $bundle, string|int $index, bool $fallback = true): object|array|string|int|null
```

PHP ext/intl `resourcebundle_get()` backed by the managed ICU bridge.

**Parameters**:
- `$bundle` (`object`)
- `$index` (`string|int`)
- `$fallback` (`bool`), default `true`, optional

**Returns**: `object|array|string|int|null`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `resourcebundle_get` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/resourcebundle_get.md).
