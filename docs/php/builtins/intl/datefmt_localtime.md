---
title: "datefmt_localtime()"
description: "PHP ext/intl `datefmt_localtime()` backed by the managed ICU bridge."
---

## datefmt_localtime()

```php
function datefmt_localtime(object $formatter, string $string, int $offset = null): array|false
```

PHP ext/intl `datefmt_localtime()` backed by the managed ICU bridge.

**Parameters**:
- `$formatter` (`object`)
- `$string` (`string`)
- `$offset` (`int`), passed by reference, default `null`, optional

**Returns**: `array|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `datefmt_localtime` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/datefmt_localtime.md).
