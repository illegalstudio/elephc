---
title: "intltz_get_offset()"
description: "PHP ext/intl `intltz_get_offset()` backed by the managed ICU bridge."
---

## intltz_get_offset()

```php
function intltz_get_offset(object $timezone, float $timestamp, bool $local, mixed $rawOffset, mixed $dstOffset): bool
```

PHP ext/intl `intltz_get_offset()` backed by the managed ICU bridge.

**Parameters**:
- `$timezone` (`object`)
- `$timestamp` (`float`)
- `$local` (`bool`)
- `$rawOffset` (`mixed`), passed by reference
- `$dstOffset` (`mixed`), passed by reference

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `intltz_get_offset` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/intltz_get_offset.md).
