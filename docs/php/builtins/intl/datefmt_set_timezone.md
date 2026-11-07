---
title: "datefmt_set_timezone()"
description: "PHP ext/intl `datefmt_set_timezone()` backed by the managed ICU bridge."
---

## datefmt_set_timezone()

```php
function datefmt_set_timezone(object $formatter, object|string|null $timezone): bool
```

PHP ext/intl `datefmt_set_timezone()` backed by the managed ICU bridge.

**Parameters**:
- `$formatter` (`object`)
- `$timezone` (`object|string|null`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `datefmt_set_timezone` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/datefmt_set_timezone.md).
