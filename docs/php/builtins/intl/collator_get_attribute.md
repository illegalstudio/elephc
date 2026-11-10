---
title: "collator_get_attribute()"
description: "PHP ext/intl `collator_get_attribute()` backed by the managed ICU bridge."
---

## collator_get_attribute()

```php
function collator_get_attribute(object $object, int $attribute): int|false
```

PHP ext/intl `collator_get_attribute()` backed by the managed ICU bridge.

**Parameters**:
- `$object` (`object`)
- `$attribute` (`int`)

**Returns**: `int|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `collator_get_attribute` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/collator_get_attribute.md).
