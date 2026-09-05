---
title: "date_timezone_set()"
description: "Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking."
sidebar:
  order: 215
---

## date_timezone_set()

```php
function date_timezone_set(mixed $object, mixed $timezone): mixed
```

Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

**Parameters**:
- `$object` (`mixed`)
- `$timezone` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_timezone_set` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_timezone_set.md).
