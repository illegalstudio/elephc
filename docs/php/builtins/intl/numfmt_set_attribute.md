---
title: "numfmt_set_attribute()"
description: "PHP ext/intl `numfmt_set_attribute()` backed by the managed ICU bridge."
---

## numfmt_set_attribute()

```php
function numfmt_set_attribute(object $formatter, int $attribute, int|float $value): bool
```

PHP ext/intl `numfmt_set_attribute()` backed by the managed ICU bridge.

**Parameters**:
- `$formatter` (`object`)
- `$attribute` (`int`)
- `$value` (`int|float`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `numfmt_set_attribute` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/numfmt_set_attribute.md).
