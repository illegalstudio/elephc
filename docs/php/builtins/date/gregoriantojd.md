---
title: "gregoriantojd()"
description: "Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking."
sidebar:
  order: 224
---

## gregoriantojd()

```php
function gregoriantojd(int $month, int $day, int $year): int
```

Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

**Parameters**:
- `$month` (`int`)
- `$day` (`int`)
- `$year` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gregoriantojd` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/gregoriantojd.md).
