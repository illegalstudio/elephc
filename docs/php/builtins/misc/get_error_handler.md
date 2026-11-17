---
title: "get_error_handler()"
description: "Returns the currently active user error handler, or null when none is installed."
---

## get_error_handler()

```php
function get_error_handler(): mixed
```

Returns the currently active user error handler, or null when none is installed.

**Parameters**: none.

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `get_error_handler` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/get_error_handler.md).
