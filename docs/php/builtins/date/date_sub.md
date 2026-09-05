---
title: "date_sub()"
description: "Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking."
sidebar:
  order: 207
---

## date_sub()

```php
function date_sub(mixed $object, mixed $interval): mixed
```

Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

**Parameters**:
- `$object` (`mixed`)
- `$interval` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_sub` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_sub.md).
