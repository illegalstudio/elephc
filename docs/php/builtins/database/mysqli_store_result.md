---
title: "mysqli_store_result()"
description: "Buffers a query's whole result on the client."
sidebar:
  order: 180
---

## mysqli_store_result()

```php
function mysqli_store_result(mixed $mysql, int $mode = 0): mixed
```

Buffers a query's whole result on the client.

**Parameters**:
- `$mysql` (`mixed`)
- `$mode` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_store_result` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_store_result.md).
