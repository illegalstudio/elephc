---
title: "numfmt_format_currency()"
description: "PHP ext/intl `numfmt_format_currency()` backed by the managed ICU bridge."
---

## numfmt_format_currency()

```php
function numfmt_format_currency(object $formatter, float $amount, string $currency): string|false
```

PHP ext/intl `numfmt_format_currency()` backed by the managed ICU bridge.

**Parameters**:
- `$formatter` (`object`)
- `$amount` (`float`)
- `$currency` (`string`)

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `numfmt_format_currency` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/numfmt_format_currency.md).
