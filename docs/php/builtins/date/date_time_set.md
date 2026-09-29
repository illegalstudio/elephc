---
title: "date_time_set()"
description: "Sets a DateTime's hour, minute, second, and microsecond."
sidebar:
  order: 214
---

## date_time_set()

```php
function date_time_set(mixed $object, int $hour, int $minute, int $second = 0, int $microsecond = 0): mixed
```

Sets a DateTime's hour, minute, second, and microsecond.

**Parameters**:
- `$object` (`mixed`)
- `$hour` (`int`)
- `$minute` (`int`)
- `$second` (`int`), default `0`, optional
- `$microsecond` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_time_set` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_time_set.md).
