---
title: "mb_get_info()"
description: "Returns the current mbstring request settings or one selected information value."
---

## mb_get_info()

```php
function mb_get_info(string $type = 'all'): array|string|int|false|null
```

Returns the current mbstring request settings or one selected information value.

**Parameters**:
- `$type` (`string`), default `'all'`, optional

**Returns**: `array|string|int|false|null`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_get_info` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_get_info.md).
