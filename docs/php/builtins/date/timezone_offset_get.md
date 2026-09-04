---
title: "timezone_offset_get()"
description: "Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking."
sidebar:
  order: 248
---

## timezone_offset_get()

```php
function timezone_offset_get(mixed $object, mixed $datetime): int
```

Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

**Parameters**:
- `$object` (`mixed`)
- `$datetime` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `timezone_offset_get` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/timezone_offset_get.md).
