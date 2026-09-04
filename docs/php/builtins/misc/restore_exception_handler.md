---
title: "restore_exception_handler()"
description: "Restores the previously active uncaught-exception handler."
sidebar:
  order: 351
---

## restore_exception_handler()

```php
function restore_exception_handler(): bool
```

Restores the previously active uncaught-exception handler.

**Parameters**: none.

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/restore_exception_handler.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/restore_exception_handler.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `restore_exception_handler` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/restore_exception_handler.md).
