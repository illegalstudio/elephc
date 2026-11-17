---
title: "mysqli_store_result()"
description: "Buffers a query's whole result on the client."
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

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mysqli_store_result` is implemented in the compiler, see [the internals page](../../../internals/builtins/database/mysqli_store_result.md).
