---
title: "mysqli_select_db()"
description: "Selects the default database for the connection."
sidebar:
  order: 157
---

## mysqli_select_db()

```php
function mysqli_select_db(mixed $mysql, string $database): bool
```

Selects the default database for the connection.

**Parameters**:
- `$mysql` (`mixed`)
- `$database` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_select_db` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_select_db.md).
