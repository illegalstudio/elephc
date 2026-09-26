---
title: "date_diff()"
description: "Returns the DateInterval between two dates."
sidebar:
  order: 200
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

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_diff` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_diff.md).
