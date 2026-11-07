---
title: "collator_get_locale()"
description: "PHP ext/intl `collator_get_locale()` backed by the managed ICU bridge."
---

## collator_get_locale()

```php
function collator_get_locale(object $object, int $type): string|false
```

PHP ext/intl `collator_get_locale()` backed by the managed ICU bridge.

**Parameters**:
- `$object` (`object`)
- `$type` (`int`)

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `collator_get_locale` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/collator_get_locale.md).
