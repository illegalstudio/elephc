---
title: "gettimeofday()"
description: "Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking."
sidebar:
  order: 220
---

## gettimeofday()

```php
function gettimeofday(bool $as_float = false): mixed
```

Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

**Parameters**:
- `$as_float` (`bool`), default `false`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gettimeofday` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/gettimeofday.md).
