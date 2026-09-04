---
title: "jdmonthname()"
description: "Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking."
sidebar:
  order: 228
---

## jdmonthname()

```php
function jdmonthname(int $julian_day, int $mode): string
```

Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

**Parameters**:
- `$julian_day` (`int`)
- `$mode` (`int`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `jdmonthname` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/jdmonthname.md).
