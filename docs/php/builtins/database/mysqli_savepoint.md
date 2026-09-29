---
title: "mysqli_savepoint()"
description: "Creates a named savepoint in the open transaction."
sidebar:
  order: 156
---

## mysqli_savepoint()

```php
function mysqli_savepoint(mixed $mysql, string $name): bool
```

Creates a named savepoint in the open transaction.

**Parameters**:
- `$mysql` (`mixed`)
- `$name` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_savepoint` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_savepoint.md).
