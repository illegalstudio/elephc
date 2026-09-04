---
title: "date_timestamp_set()"
description: "Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking."
sidebar:
  order: 213
---

## date_timestamp_set()

```php
function date_timestamp_set(mixed $object, int $timestamp): mixed
```

Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

**Parameters**:
- `$object` (`mixed`)
- `$timestamp` (`int`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_timestamp_set` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_timestamp_set.md).
