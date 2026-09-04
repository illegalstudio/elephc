---
title: "cal_from_jd()"
description: "Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking."
sidebar:
  order: 184
---

## cal_from_jd()

```php
function cal_from_jd(int $julian_day, int $calendar): mixed
```

Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

**Parameters**:
- `$julian_day` (`int`)
- `$calendar` (`int`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `cal_from_jd` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/cal_from_jd.md).
