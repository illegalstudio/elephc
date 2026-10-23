---
title: "mysqli_begin_transaction()"
description: "Starts a transaction."
sidebar:
  order: 102
---

## mysqli_begin_transaction()

```php
function mysqli_begin_transaction(mixed $mysql, int $flags = 0, ?string $name = null): bool
```

Starts a transaction.

**Parameters**:
- `$mysql` (`mixed`)
- `$flags` (`int`), default `0`, optional
- `$name` (`?string`), default `null`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected mysqli prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_begin_transaction` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_begin_transaction.md).
