---
title: "json_last_error_msg()"
description: "Returns the error string of the last json_encode() or json_decode() call."
---

## json_last_error_msg()

```php
function json_last_error_msg(): string
```

Returns the error string of the last json_encode() or json_decode() call.

**Parameters**: none.

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `json_last_error_msg` is implemented in the compiler, see [the internals page](../../../internals/builtins/json/json_last_error_msg.md).
