---
title: "restore_exception_handler()"
description: "Restores the previously active uncaught-exception handler."
---

## restore_exception_handler()

```php
function restore_exception_handler(): bool
```

Restores the previously active uncaught-exception handler.

**Parameters**: none.

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `restore_exception_handler` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/restore_exception_handler.md).
