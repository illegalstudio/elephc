---
title: "mysqli_reap_async_query()"
description: "Returns the result of an async query."
sidebar:
  order: 164
---

## mysqli_reap_async_query()

```php
function mysqli_reap_async_query(mixed $mysql): mixed
```

Returns the result of an async query.

**Parameters**:
- `$mysql` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_reap_async_query` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_reap_async_query.md).
