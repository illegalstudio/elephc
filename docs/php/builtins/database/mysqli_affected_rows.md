---
title: "mysqli_affected_rows()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 98
---

## mysqli_affected_rows()

```php
function mysqli_affected_rows(mixed $mysql): int
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_affected_rows` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_affected_rows.md).
