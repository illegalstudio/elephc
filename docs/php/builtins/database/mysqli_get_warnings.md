---
title: "mysqli_get_warnings()"
description: "Returns the warnings of the last statement."
sidebar:
  order: 145
---

## mysqli_get_warnings()

```php
function mysqli_get_warnings(mixed $mysql): mixed
```

Returns the warnings of the last statement.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_get_warnings` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_get_warnings.md).
