---
title: "date_diff()"
description: "Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking."
sidebar:
  order: 197
---

## date_diff()

```php
function date_diff(mixed $baseObject, mixed $targetObject, bool $absolute = false): mixed
```

Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

**Parameters**:
- `$baseObject` (`mixed`)
- `$targetObject` (`mixed`)
- `$absolute` (`bool`), default `false`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_diff` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_diff.md).
