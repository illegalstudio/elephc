---
title: "numfmt_parse_currency()"
description: "PHP ext/intl `numfmt_parse_currency()` backed by the managed ICU bridge."
---

## numfmt_parse_currency()

```php
function numfmt_parse_currency(object $formatter, string $string, mixed $currency, int $offset = null): float|false
```

PHP ext/intl `numfmt_parse_currency()` backed by the managed ICU bridge.

**Parameters**:
- `$formatter` (`object`)
- `$string` (`string`)
- `$currency` (`mixed`), passed by reference
- `$offset` (`int`), passed by reference, default `null`, optional

**Returns**: `float|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `numfmt_parse_currency` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/numfmt_parse_currency.md).
