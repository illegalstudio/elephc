---
title: "date_diff()"
description: "Returns the DateInterval between two dates."
---

## date_diff()

```php
function date_diff(mixed $baseObject, mixed $targetObject, bool $absolute = false): mixed
```

Returns the DateInterval between two dates.

**Parameters**:
- `$baseObject` (`mixed`)
- `$targetObject` (`mixed`)
- `$absolute` (`bool`), default `false`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_diff` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_diff.md).
