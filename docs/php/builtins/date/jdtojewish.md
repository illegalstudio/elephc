---
title: "jdtojewish()"
description: "Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking."
sidebar:
  order: 231
---

## jdtojewish()

```php
function jdtojewish(int $julian_day, bool $hebrew = false, int $flags = 0): string
```

Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

**Parameters**:
- `$julian_day` (`int`)
- `$hebrew` (`bool`), default `false`, optional
- `$flags` (`int`), default `0`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `jdtojewish` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/jdtojewish.md).
