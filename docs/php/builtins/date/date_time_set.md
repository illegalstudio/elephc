---
title: "date_time_set()"
description: "Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking."
sidebar:
  order: 211
---

## date_time_set()

```php
function date_time_set(mixed $object, int $hour, int $minute, int $second = 0, int $microsecond = 0): mixed
```

Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

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

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_time_set` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_time_set.md).
