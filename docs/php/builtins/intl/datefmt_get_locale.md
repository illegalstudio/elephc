---
title: "datefmt_get_locale()"
description: "PHP ext/intl `datefmt_get_locale()` backed by the managed ICU bridge."
---

## datefmt_get_locale()

```php
function datefmt_get_locale(object $formatter, int $type = 0): string|false
```

PHP ext/intl `datefmt_get_locale()` backed by the managed ICU bridge.

**Parameters**:
- `$formatter` (`object`)
- `$type` (`int`), default `0`, optional

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `datefmt_get_locale` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/datefmt_get_locale.md).
