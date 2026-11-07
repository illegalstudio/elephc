---
title: "intltz_get_equivalent_id()"
description: "PHP ext/intl `intltz_get_equivalent_id()` backed by the managed ICU bridge."
---

## intltz_get_equivalent_id()

```php
function intltz_get_equivalent_id(string $timezoneId, int $offset): string|false
```

PHP ext/intl `intltz_get_equivalent_id()` backed by the managed ICU bridge.

**Parameters**:
- `$timezoneId` (`string`)
- `$offset` (`int`)

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `intltz_get_equivalent_id` is implemented in the compiler, see [the internals page](../../../internals/builtins/intl/intltz_get_equivalent_id.md).
