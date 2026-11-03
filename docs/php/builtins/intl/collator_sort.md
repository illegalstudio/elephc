---
title: "collator_sort()"
description: "PHP ext/intl `collator_sort()` backed by the managed ICU bridge."
---

## collator_sort()

```php
function collator_sort(object $object, array $array, int $flags = Collator::SORT_REGULAR): bool
```

PHP ext/intl `collator_sort()` backed by the managed ICU bridge.

**Parameters**:
- `$object` (`object`)
- `$array` (`array`), passed by reference
- `$flags` (`int`), default `Collator::SORT_REGULAR`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `collator_sort` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/collator_sort.md).
