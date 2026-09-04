---
title: "date_isodate_set()"
description: "Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking."
sidebar:
  order: 202
---

## date_isodate_set()

```php
function date_isodate_set(mixed $object, int $year, int $week, int $dayOfWeek = 1): mixed
```

Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

**Parameters**:
- `$object` (`mixed`)
- `$year` (`int`)
- `$week` (`int`)
- `$dayOfWeek` (`int`), default `1`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_isodate_set` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_isodate_set.md).
