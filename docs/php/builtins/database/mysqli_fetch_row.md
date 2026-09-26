---
title: "mysqli_fetch_row()"
description: "Returns the next row as a numeric array."
sidebar:
  order: 126
---

## mysqli_fetch_row()

```php
function mysqli_fetch_row(mixed $result): ?array
```

Returns the next row as a numeric array.

**Parameters**:
- `$result` (`mixed`)

**Returns**: `?array`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_fetch_row` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_fetch_row.md).
