---
title: "collator_asort()"
description: "PHP ext/intl `collator_asort()` backed by the managed ICU bridge."
---

## collator_asort()

```php
function collator_asort(object $object, array $array, int $flags = Collator::SORT_REGULAR): bool
```

PHP ext/intl `collator_asort()` backed by the managed ICU bridge.

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

For how `collator_asort` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/collator_asort.md).
