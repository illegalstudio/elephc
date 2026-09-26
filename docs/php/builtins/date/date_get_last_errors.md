---
title: "date_get_last_errors()"
description: "Returns the warnings and errors from the last date parse."
sidebar:
  order: 202
---

## date_get_last_errors()

```php
function date_get_last_errors(): mixed
```

Returns the warnings and errors from the last date parse.

**Parameters**: none.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the Elephc compiler.
- **`eval()` (magician interpreter)**: supported through the procedural date/time alias dispatcher.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `date_get_last_errors` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_get_last_errors.md).
