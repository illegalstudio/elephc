---
title: "mysqli_query()"
description: "Implemented by the compiler-injected mysqli prelude."
sidebar:
  order: 146
---

## mysqli_query()

```php
function mysqli_query(mixed $mysql, string $query, int $result_mode = 0): mixed
```

Implemented by the compiler-injected mysqli prelude.

**Parameters**:
- `$mysql` (`mixed`)
- `$query` (`string`)
- `$result_mode` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_query` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_query.md).
