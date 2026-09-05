---
title: "date_timestamp_get()"
description: "Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking."
sidebar:
  order: 212
---

## date_timestamp_get()

```php
function date_timestamp_get(mixed $object): int
```

Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

**Parameters**:
- `$object` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_timestamp_get` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_timestamp_get.md).
