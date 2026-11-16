---
title: "call_user_func()"
description: "Calls a callback with the given arguments."
---

## call_user_func()

```php
function call_user_func(callable $callback, ...$args): mixed
```

Calls a callback with the given arguments.

**Parameters**:
- `$callback` (`callable`)
- `...$args` - variadic: collects excess arguments into `$args`.

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `call_user_func` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/call_user_func.md).
