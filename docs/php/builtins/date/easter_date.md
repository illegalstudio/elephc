---
title: "easter_date()"
description: "Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking."
sidebar:
  order: 216
---

## easter_date()

```php
function easter_date(int $year = null, int $mode = CAL_EASTER_DEFAULT): int
```

Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

**Parameters**:
- `$year` (`int`), default `null`, optional
- `$mode` (`int`), default `CAL_EASTER_DEFAULT`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `easter_date` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/easter_date.md).
