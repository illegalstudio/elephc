---
title: "mysqli_use_result()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 180
---

## mysqli_use_result()

```php
function mysqli_use_result(mixed $mysql): mixed
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_use_result` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_use_result.md).
