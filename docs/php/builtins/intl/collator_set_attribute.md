---
title: "collator_set_attribute()"
description: "PHP ext/intl `collator_set_attribute()` backed by the managed ICU bridge."
---

## collator_set_attribute()

```php
function collator_set_attribute(object $object, int $attribute, int $value): bool
```

PHP ext/intl `collator_set_attribute()` backed by the managed ICU bridge.

**Parameters**:
- `$object` (`object`)
- `$attribute` (`int`)
- `$value` (`int`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `collator_set_attribute` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/collator_set_attribute.md).
