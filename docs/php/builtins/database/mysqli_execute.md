---
title: "mysqli_execute()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 112
---

## mysqli_execute()

```php
function mysqli_execute(mixed $statement): bool
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$statement` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_execute` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_execute.md).
