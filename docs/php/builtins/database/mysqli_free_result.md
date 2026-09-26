---
title: "mysqli_free_result()"
description: "Releases the memory a result set holds."
sidebar:
  order: 130
---

## mysqli_free_result()

```php
function mysqli_free_result(mixed $result): void
```

Releases the memory a result set holds.

**Parameters**:
- `$result` (`mixed`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_free_result` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_free_result.md).
