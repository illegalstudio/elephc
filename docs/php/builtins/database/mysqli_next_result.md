---
title: "mysqli_next_result()"
description: "Advances a multi-query to its next result set."
sidebar:
  order: 143
---

## mysqli_next_result()

```php
function mysqli_next_result(mixed $mysql): bool
```

Advances a multi-query to its next result set.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_next_result` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_next_result.md).
