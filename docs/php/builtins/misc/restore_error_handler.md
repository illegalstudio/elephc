---
title: "restore_error_handler()"
description: "Restores the previously active user error handler."
sidebar:
  order: 350
---

## restore_error_handler()

```php
function restore_error_handler(): bool
```

Restores the previously active user error handler.

**Parameters**: none.

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/restore_error_handler.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/restore_error_handler.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `restore_error_handler` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/restore_error_handler.md).
