---
title: "numfmt_parse()"
description: "PHP ext/intl `numfmt_parse()` backed by the managed ICU bridge."
---

## numfmt_parse()

```php
function numfmt_parse(object $formatter, string $string, int $type = NumberFormatter::TYPE_DOUBLE, int $offset = null): int|float|false
```

PHP ext/intl `numfmt_parse()` backed by the managed ICU bridge.

**Parameters**:
- `$formatter` (`object`)
- `$string` (`string`)
- `$type` (`int`), default `NumberFormatter::TYPE_DOUBLE`, optional
- `$offset` (`int`), passed by reference, default `null`, optional

**Returns**: `int|float|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `numfmt_parse` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/numfmt_parse.md).
