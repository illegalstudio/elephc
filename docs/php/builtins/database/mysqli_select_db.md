---
title: "mysqli_select_db()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 154
---

## mysqli_select_db()

```php
function mysqli_select_db(mixed $mysql, string $database): bool
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$mysql` (`mixed`)
- `$database` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_select_db` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_select_db.md).
