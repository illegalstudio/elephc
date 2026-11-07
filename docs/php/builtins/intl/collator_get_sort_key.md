---
title: "collator_get_sort_key()"
description: "PHP ext/intl `collator_get_sort_key()` backed by the managed ICU bridge."
---

## collator_get_sort_key()

```php
function collator_get_sort_key(object $object, string $string): string|false
```

PHP ext/intl `collator_get_sort_key()` backed by the managed ICU bridge.

**Parameters**:
- `$object` (`object`)
- `$string` (`string`)

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `collator_get_sort_key` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/collator_get_sort_key.md).
