---
title: "mysqli_multi_query()"
description: "Runs several semicolon-separated statements in one call."
sidebar:
  order: 142
---

## mysqli_multi_query()

```php
function mysqli_multi_query(mixed $mysql, string $query): bool
```

Runs several semicolon-separated statements in one call.

**Parameters**:
- `$mysql` (`mixed`)
- `$query` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_multi_query` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_multi_query.md).
