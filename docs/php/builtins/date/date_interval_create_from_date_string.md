---
title: "date_interval_create_from_date_string()"
description: "Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking."
sidebar:
  order: 200
---

## date_interval_create_from_date_string()

```php
function date_interval_create_from_date_string(string $datetime): mixed
```

Rewritten by the name resolver into a constructor or method call on the corresponding builtin class before type checking.

**Parameters**:
- `$datetime` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_interval_create_from_date_string` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_interval_create_from_date_string.md).
