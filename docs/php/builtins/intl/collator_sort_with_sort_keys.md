---
title: "collator_sort_with_sort_keys()"
description: "PHP ext/intl `collator_sort_with_sort_keys()` backed by the managed ICU bridge."
---

## collator_sort_with_sort_keys()

```php
function collator_sort_with_sort_keys(object $object, array $array): bool
```

PHP ext/intl `collator_sort_with_sort_keys()` backed by the managed ICU bridge.

**Parameters**:
- `$object` (`object`)
- `$array` (`array`), passed by reference

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `collator_sort_with_sort_keys` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/collator_sort_with_sort_keys.md).
