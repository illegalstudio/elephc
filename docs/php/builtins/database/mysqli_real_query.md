---
title: "mysqli_real_query()"
description: "Runs one statement without fetching its result."
sidebar:
  order: 151
---

## mysqli_real_query()

```php
function mysqli_real_query(mixed $mysql, string $query): bool
```

Runs one statement without fetching its result.

**Parameters**:
- `$mysql` (`mixed`)
- `$query` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected mysqli prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_real_query` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_real_query.md).
