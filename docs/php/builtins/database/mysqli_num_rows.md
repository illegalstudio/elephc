---
title: "mysqli_num_rows()"
description: "Returns how many rows a result has."
sidebar:
  order: 145
---

## mysqli_num_rows()

```php
function mysqli_num_rows(mixed $result): int
```

Returns how many rows a result has.

**Parameters**:
- `$result` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_num_rows` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_num_rows.md).
