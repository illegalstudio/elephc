---
title: "mysqli_release_savepoint()"
description: "Removes a named savepoint from the open transaction."
sidebar:
  order: 152
---

## mysqli_release_savepoint()

```php
function mysqli_release_savepoint(mixed $mysql, string $name): bool
```

Removes a named savepoint from the open transaction.

**Parameters**:
- `$mysql` (`mixed`)
- `$name` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected mysqli prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_release_savepoint` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_release_savepoint.md).
