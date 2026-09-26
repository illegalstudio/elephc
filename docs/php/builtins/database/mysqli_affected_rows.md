---
title: "mysqli_affected_rows()"
description: "Returns how many rows the last write affected."
sidebar:
  order: 101
---

## mysqli_affected_rows()

```php
function mysqli_affected_rows(mixed $mysql): int
```

Returns how many rows the last write affected.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_affected_rows` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_affected_rows.md).
