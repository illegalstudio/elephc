---
title: "collator_compare()"
description: "PHP ext/intl `collator_compare()` backed by the managed ICU bridge."
---

## collator_compare()

```php
function collator_compare(object $object, string $string1, string $string2): int|false
```

PHP ext/intl `collator_compare()` backed by the managed ICU bridge.

**Parameters**:
- `$object` (`object`)
- `$string1` (`string`)
- `$string2` (`string`)

**Returns**: `int|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `collator_compare` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/collator_compare.md).
