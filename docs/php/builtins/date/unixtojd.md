---
title: "unixtojd()"
description: "Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking."
sidebar:
  order: 252
---

## unixtojd()

```php
function unixtojd(?int $timestamp = null): mixed
```

Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

**Parameters**:
- `$timestamp` (`?int`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `unixtojd` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/unixtojd.md).
