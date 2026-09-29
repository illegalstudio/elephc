---
title: "get_exception_handler()"
description: "Returns the currently active uncaught-exception handler, or null when none is installed."
sidebar:
  order: 627
---

## get_exception_handler()

```php
function get_exception_handler(): mixed
```

Returns the currently active uncaught-exception handler, or null when none is installed.

**Parameters**: none.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/get_exception_handler.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/get_exception_handler.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `get_exception_handler` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/get_exception_handler.md).
