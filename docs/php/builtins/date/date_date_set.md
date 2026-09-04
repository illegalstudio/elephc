---
title: "date_date_set()"
description: "Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking."
sidebar:
  order: 194
---

## date_date_set()

```php
function date_date_set(mixed $object, int $year, int $month, int $day): mixed
```

Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

**Parameters**:
- `$object` (`mixed`)
- `$year` (`int`)
- `$month` (`int`)
- `$day` (`int`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_date_set` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_date_set.md).
