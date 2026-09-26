---
title: "mysqli_fetch_lengths()"
description: "Returns the byte lengths of the columns in the current row."
sidebar:
  order: 124
---

## mysqli_fetch_lengths()

```php
function mysqli_fetch_lengths(mixed $result): ?array
```

Returns the byte lengths of the columns in the current row.

**Parameters**:
- `$result` (`mixed`)

**Returns**: `?array`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_fetch_lengths` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_fetch_lengths.md).
