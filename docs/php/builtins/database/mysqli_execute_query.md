---
title: "mysqli_execute_query()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 113
---

## mysqli_execute_query()

```php
function mysqli_execute_query(mixed $mysql, string $query, mixed $params = null): mixed
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$mysql` (`mixed`)
- `$query` (`string`)
- `$params` (`mixed`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_execute_query` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_execute_query.md).
