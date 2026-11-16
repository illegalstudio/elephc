---
title: "timezone_transitions_get()"
description: "Returns a timezone's DST transitions in a time range."
---

## timezone_transitions_get()

```php
function timezone_transitions_get(mixed $object, int $timestampBegin = PHP_INT_MIN, int $timestampEnd = PHP_INT_MAX): mixed
```

Returns a timezone's DST transitions in a time range.

**Parameters**:
- `$object` (`mixed`)
- `$timestampBegin` (`int`), default `PHP_INT_MIN`, optional
- `$timestampEnd` (`int`), default `PHP_INT_MAX`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `timezone_transitions_get` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/timezone_transitions_get.md).
