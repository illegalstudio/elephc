---
title: "set_exception_handler()"
description: "Installs an uncaught-exception handler and returns the previous handler."
---

## set_exception_handler()

```php
function set_exception_handler(mixed $callback): mixed
```

Installs an uncaught-exception handler and returns the previous handler.

**Parameters**:
- `$callback` (`mixed`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `set_exception_handler` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/set_exception_handler.md).
