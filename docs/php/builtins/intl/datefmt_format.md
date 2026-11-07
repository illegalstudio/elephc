---
title: "datefmt_format()"
description: "PHP ext/intl `datefmt_format()` backed by the managed ICU bridge."
---

## datefmt_format()

```php
function datefmt_format(object $formatter, mixed $datetime): string|false
```

PHP ext/intl `datefmt_format()` backed by the managed ICU bridge.

**Parameters**:
- `$formatter` (`object`)
- `$datetime` (`mixed`)

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `datefmt_format` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/datefmt_format.md).
