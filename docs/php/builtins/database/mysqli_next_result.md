---
title: "mysqli_next_result()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 140
---

## mysqli_next_result()

```php
function mysqli_next_result(mixed $mysql): bool
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_next_result` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_next_result.md).
