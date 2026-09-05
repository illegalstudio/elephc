---
title: "date_create()"
description: "Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking."
sidebar:
  order: 190
---

## date_create()

```php
function date_create(string $datetime = 'now', mixed $timezone = null): mixed
```

Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

**Parameters**:
- `$datetime` (`string`), default `'now'`, optional
- `$timezone` (`mixed`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_create` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_create.md).
