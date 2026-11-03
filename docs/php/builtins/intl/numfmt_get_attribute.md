---
title: "numfmt_get_attribute()"
description: "PHP ext/intl `numfmt_get_attribute()` backed by the managed ICU bridge."
---

## numfmt_get_attribute()

```php
function numfmt_get_attribute(object $formatter, int $attribute): int|float|false
```

PHP ext/intl `numfmt_get_attribute()` backed by the managed ICU bridge.

**Parameters**:
- `$formatter` (`object`)
- `$attribute` (`int`)

**Returns**: `int|float|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `numfmt_get_attribute` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/numfmt_get_attribute.md).
