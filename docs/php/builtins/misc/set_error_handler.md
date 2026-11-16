---
title: "set_error_handler()"
description: "Installs a user error handler and returns the previous handler."
---

## set_error_handler()

```php
function set_error_handler(mixed $callback, int $error_levels = E_ALL): mixed
```

Installs a user error handler and returns the previous handler.

**Parameters**:
- `$callback` (`mixed`)
- `$error_levels` (`int`), default `E_ALL`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `set_error_handler` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/set_error_handler.md).
