---
title: "numfmt_format()"
description: "PHP ext/intl `numfmt_format()` backed by the managed ICU bridge."
---

## numfmt_format()

```php
function numfmt_format(object $formatter, int|float $num, int $type = NumberFormatter::TYPE_DEFAULT): string|false
```

PHP ext/intl `numfmt_format()` backed by the managed ICU bridge.

**Parameters**:
- `$formatter` (`object`)
- `$num` (`int|float`)
- `$type` (`int`), default `NumberFormatter::TYPE_DEFAULT`, optional

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `numfmt_format` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/numfmt_format.md).
