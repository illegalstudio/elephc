---
title: "mysqli_sqlstate()"
description: "Returns the SQLSTATE of the last call on a connection."
sidebar:
  order: 160
---

## mysqli_sqlstate()

```php
function mysqli_sqlstate(mixed $mysql): string
```

Returns the SQLSTATE of the last call on a connection.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_sqlstate` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_sqlstate.md).
