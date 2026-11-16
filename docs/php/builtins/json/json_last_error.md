---
title: "json_last_error()"
description: "Returns the last error (if any) occurred during the last JSON encoding/decoding."
---

## json_last_error()

```php
function json_last_error(): int
```

Returns the last error (if any) occurred during the last JSON encoding/decoding.

**Parameters**: none.

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `json_last_error` is implemented in the compiler, see [the internals page](../../../internals/builtins/json/json_last_error.md).
