---
title: "mysqli_fetch_column()"
description: "Returns one column of the next row."
sidebar:
  order: 120
---

## mysqli_fetch_column()

```php
function mysqli_fetch_column(mixed $result, int $column = 0): mixed
```

Returns one column of the next row.

**Parameters**:
- `$result` (`mixed`)
- `$column` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_fetch_column` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_fetch_column.md).
