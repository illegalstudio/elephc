---
title: "call_user_func_array()"
description: "Calls a callback with an array of parameters."
---

## call_user_func_array()

```php
function call_user_func_array(callable $callback, array $args): mixed
```

Calls a callback with an array of parameters.

**Parameters**:
- `$callback` (`callable`)
- `$args` (`array`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `call_user_func_array` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/call_user_func_array.md).
