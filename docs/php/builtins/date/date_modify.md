---
title: "date_modify()"
description: "Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking."
sidebar:
  order: 203
---

## date_modify()

```php
function date_modify(mixed $object, string $modifier): mixed
```

Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

**Parameters**:
- `$object` (`mixed`)
- `$modifier` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_modify` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_modify.md).
