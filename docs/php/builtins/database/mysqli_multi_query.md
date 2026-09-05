---
title: "mysqli_multi_query()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 139
---

## mysqli_multi_query()

```php
function mysqli_multi_query(mixed $mysql, string $query): bool
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$mysql` (`mixed`)
- `$query` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_multi_query` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_multi_query.md).
